//! Fuse taking the last element of a slice with unwrapping the result.

use keel_ir::{
    BinaryOp, Block, BlockCall, EntityRef, Function, InsertPoint, Inst, InstData, IntCC, Module,
    Type, Value,
};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Default)]
pub(crate) struct CallHints {
    /// The exact element stride from the instantiated Rust signature.
    pub(crate) last: FxHashMap<String, u64>,
    pub(crate) unwrap: FxHashSet<String>,
}

pub(crate) fn fuse(module: &mut Module, hints: &CallHints) {
    if hints.last.is_empty() || hints.unwrap.is_empty() || !module.files.is_empty() {
        return;
    }
    for function in &mut module.functions {
        if function.attrs.optnone
            || function.attrs.naked
            || function.attrs.interrupt
            || function.sig.returns_twice
            || function.personality.is_some()
            || function.debug_loc.is_some()
            || !function.debug_vars.is_empty()
            || !function
                .callees()
                .any(|(_, callee)| hints.last.contains_key(&callee.name))
        {
            continue;
        }
        let view = &*function;
        let sites =
            view.blocks()
                .flat_map(|block| {
                    view.block_insts(block).windows(3).enumerate().filter_map(
                        move |(index, window)| Site::find(view, hints, block, index, window),
                    )
                })
                .collect::<Vec<_>>();
        if sites.is_empty() {
            continue;
        }
        let mut uses = vec![0u32; function.num_values()];
        for (_, inst) in function.layout_insts() {
            function.inst_data(inst).for_each_operand(|value| {
                let value = function.resolve(value);
                uses[value.index()] = uses[value.index()].saturating_add(1);
            });
        }
        for site in sites.into_iter().rev() {
            if site.inputs_used_once(function, &uses) {
                site.rewrite(function);
            }
        }
    }
}

struct Site {
    block: Block,
    index: usize,
    last: Inst,
    location: Inst,
    unwrap: Inst,
    data: Value,
    len: Value,
    stride: i64,
}

impl Site {
    fn find(
        f: &Function,
        hints: &CallHints,
        block: Block,
        index: usize,
        window: &[Inst],
    ) -> Option<Self> {
        let [last, location, unwrap] = *window else {
            return None;
        };
        let InstData::Call { func, args } = f.inst_data(last) else {
            return None;
        };
        let stride = i64::try_from(*hints.last.get(&f.callee(*func).name)?).ok()?;
        let [data, len] = **args else { return None };
        if args.has_explicit_abi()
            || f.value_type(data) != Type::Ptr
            || f.value_type(len) != Type::I64
        {
            return None;
        }
        let [tag, payload] = *f.inst_results(last) else {
            return None;
        };
        if !matches!(f.inst_data(location), InstData::SymbolAddr { .. }) {
            return None;
        }
        let InstData::Call { func, args } = f.inst_data(unwrap) else {
            return None;
        };
        let [tag_arg, payload_arg, loc] = **args else {
            return None;
        };
        if args.has_explicit_abi()
            || !hints.unwrap.contains(&f.callee(*func).name)
            || f.resolve(tag_arg) != tag
            || f.resolve(payload_arg) != payload
            || f.resolve(loc) != f.first_result(location)
            || [last, location, unwrap].iter().any(|&inst| {
                f.inst_loc(inst).is_some()
                    || f.inst_facts(inst).is_some()
                    || f.call_contract(inst).is_some()
            })
        {
            return None;
        }
        let [result] = f.inst_results(unwrap) else {
            return None;
        };
        if f.value_type(*result) != Type::Ptr {
            return None;
        }
        Some(Self {
            block,
            index,
            last,
            location,
            unwrap,
            data,
            len,
            stride,
        })
    }

    fn inputs_used_once(&self, f: &Function, uses: &[u32]) -> bool {
        f.inst_results(self.last)
            .iter()
            .copied()
            .chain([f.first_result(self.location)])
            .all(|value| uses[value.index()] == 1)
    }

    fn rewrite(self, f: &mut Function) {
        let Self {
            block,
            index,
            last,
            location,
            unwrap,
            data,
            len,
            stride,
        } = self;
        let fast = f.create_block();
        let cold = f.create_block();
        let join = f.create_block();
        let value = f.append_block_param(join, Type::Ptr);

        let facts = f.block_insts(block)[index + 3..]
            .iter()
            .filter_map(|&inst| f.inst_facts(inst).map(|&facts| (inst, facts)))
            .collect::<Vec<_>>();
        f.move_insts_to_end(block, index + 3, join);
        for (inst, facts) in facts {
            f.set_inst_facts(inst, facts)
                .expect("moved instruction still supports its facts");
        }

        // Keep the original calls on the None path, including unwrap's caller location.
        f.move_inst(last, InsertPoint::End(cold));
        f.move_inst(location, InsertPoint::End(cold));
        let unwrap_data = f.inst_data(unwrap).clone();
        let cold_value = emit(f, cold, unwrap_data);
        let result = f.first_result(unwrap);
        f.replace_all_uses(result, value);
        f.remove_inst(unwrap);
        jump(f, cold, join, cold_value);

        let zero = emit(f, block, InstData::iconst(Type::I64, 0));
        let empty = emit(
            f,
            block,
            InstData::Icmp {
                cond: IntCC::Eq,
                args: [len, zero],
            },
        );
        f.append_inst(
            block,
            InstData::Brif {
                cond: empty,
                dests: Box::new([
                    BlockCall {
                        block: cold,
                        args: vec![],
                    },
                    BlockCall {
                        block: fast,
                        args: vec![],
                    },
                ]),
            },
        );
        let address = if stride == 0 {
            data
        } else {
            let one = emit(f, fast, InstData::iconst(Type::I64, 1));
            let index = emit(
                f,
                fast,
                InstData::Binary {
                    op: BinaryOp::Isub,
                    args: [len, one],
                },
            );
            let offset = if stride == 1 {
                index
            } else {
                let (op, constant) = if (stride as u64).is_power_of_two() {
                    (BinaryOp::Shl, i64::from(stride.trailing_zeros()))
                } else {
                    (BinaryOp::Imul, stride)
                };
                let bytes = emit(f, fast, InstData::iconst(Type::I64, constant));
                emit(
                    f,
                    fast,
                    InstData::Binary {
                        op,
                        args: [index, bytes],
                    },
                )
            };
            emit(
                f,
                fast,
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args: [data, offset],
                },
            )
        };
        jump(f, fast, join, address);
    }
}

fn emit(f: &mut Function, block: Block, inst: InstData) -> Value {
    let inst = f.append_inst(block, inst);
    f.first_result(inst)
}

fn jump(f: &mut Function, block: Block, dest: Block, value: Value) {
    f.append_inst(
        block,
        InstData::Jump {
            dest: BlockCall {
                block: dest,
                args: vec![value],
            },
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_and_unwrap_keep_the_caller_location_on_empty_slices() {
        let mut module = keel_ir::parse(
            r#"
            data @location = const align 1 { b"caller" }
            decl @last(ptr,i64) -> (i64,ptr)
            decl @unwrap(i64,ptr,ptr) -> ptr
            func @read(ptr,i64) -> i32 {
            entry(v0:ptr,v1:i64):
                v2,v3 = call @last(v0,v1)
                v4 = symbol_addr @location
                v5 = call @unwrap(v2,v3,v4)
                v6 = load.i32 v5
                return v6
            }
            "#,
        )
        .unwrap();
        let hints = CallHints {
            last: FxHashMap::from_iter([("last".to_owned(), 12)]),
            unwrap: FxHashSet::from_iter(["unwrap".to_owned()]),
        };
        keel_ir::verify_module(&module).unwrap();
        fuse(&mut module, &hints);
        keel_ir::verify_module(&module).unwrap();

        let function = &module.functions[0];
        let entry = function.entry_block().unwrap();
        let terminator = *function.block_insts(entry).last().unwrap();
        let InstData::Brif { dests, .. } = function.inst_data(terminator) else {
            panic!("the fused entry must test for an empty slice");
        };
        let [last, location, unwrap, _] = function.block_insts(dests[0].block) else {
            panic!("the empty path must retain both calls and the caller location");
        };
        assert!(matches!(function.inst_data(*last), InstData::Call { .. }));
        assert!(matches!(
            function.inst_data(*location),
            InstData::SymbolAddr { .. }
        ));
        assert!(matches!(function.inst_data(*unwrap), InstData::Call { .. }));
        assert!(
            function.block_insts(dests[1].block).iter().any(|&inst| {
                matches!(function.inst_data(inst), InstData::Iconst { imm: 12, .. })
            })
        );
        assert!(
            function
                .block_insts(dests[1].block)
                .iter()
                .all(|&inst| { !matches!(function.inst_data(inst), InstData::Call { .. }) })
        );
    }
}
