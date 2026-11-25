//! Resolve calls through known immutable local vtable slots.
use keel_ir::fxhash::FxHashMap;
use keel_ir::{BinaryOp, DataItem, Function, InstData, Module, Symbol, Type, Value, ValueDef};

pub(crate) fn run(module: &mut Module) -> bool {
    let tables: FxHashMap<_, _> = module
        .data
        .iter()
        .filter_map(|object| {
            let init = object.init.as_ref()?;
            (object.local
                && !object.weak
                && !object.weak_odr
                && !object.thread_local
                && object.section.is_none()
                && init.constant
                && init.align >= 8
                && init.items.iter().all(|item| {
                    matches!(
                        item,
                        DataItem::Ptr { .. }
                            | DataItem::Zero(8)
                            | DataItem::Int { ty: Type::I64, .. }
                    )
                }))
            .then_some((object.name.as_str(), &init.items))
        })
        .collect();
    if tables.is_empty() || !module.assembly.is_empty() {
        return false;
    }
    let definitions: FxHashMap<_, _> = module
        .decls
        .iter()
        .map(|decl| (decl.name.clone(), (decl.sig.clone(), decl.attrs.clone())))
        .chain(
            module
                .functions
                .iter()
                .map(|f| (f.name.clone(), (f.sig.clone(), f.attrs.clone()))),
        )
        .collect();
    let small: keel_ir::fxhash::FxHashSet<_> = module
        .functions
        .iter()
        .filter(|f| f.blocks().take(5).count() < 5 && f.layout_insts().take(65).count() < 65)
        .map(|f| f.name.clone())
        .collect();
    let mut changed = false;
    for function in &mut module.functions {
        if function.attrs.optnone
            || function.attrs.naked
            || function.attrs.interrupt
            || function
                .layout_insts()
                .any(|(_, inst)| matches!(function.inst_data(inst), InstData::InlineAsm { .. }))
        {
            continue;
        }
        let candidates: Vec<_> = function
            .layout_insts()
            .filter_map(|(_, inst)| {
                let InstData::CallIndirect { sig, callee, .. } = function.inst_data(inst) else {
                    return None;
                };
                let InstData::Load {
                    ty: Type::Ptr,
                    addr,
                    flags,
                } = def(function, *callee)?
                else {
                    return None;
                };
                if flags.volatile || flags.align > 8 {
                    return None;
                }
                let (table, offset) = match def(function, *addr)? {
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        args: [table, offset],
                    } => {
                        let InstData::Iconst { imm, .. } = def(function, *offset)? else {
                            return None;
                        };
                        (*table, usize::try_from(*imm).ok()?)
                    }
                    _ => (*addr, 0),
                };
                let InstData::SymbolAddr {
                    symbol: symbol @ Symbol::Data(_),
                } = def(function, table)?
                else {
                    return None;
                };
                if offset % 8 != 0 {
                    return None;
                }
                let DataItem::Ptr {
                    symbol: name,
                    addend: 0,
                } = tables.get(function.symbol_name(*symbol))?.get(offset / 8)?
                else {
                    return None;
                };
                let target = definitions.get(name)?;
                if &target.0 != function.signature(*sig) {
                    return None;
                }
                let existing = function.callees().find(|(_, decl)| decl.name == *name);
                if existing.is_some_and(|(_, decl)| decl.sig != target.0) {
                    return None;
                }
                Some((inst, name, target, existing.map(|(func, _)| func)))
            })
            .collect();
        for (inst, name, (sig, attrs), existing) in candidates {
            let func = existing.unwrap_or_else(|| function.import_function(name, sig.clone()));
            let mut attrs = if existing.is_some() {
                function.callee(func).attrs.clone()
            } else {
                attrs.clone()
            };
            if small.contains(name) && !attrs.noinline && !attrs.optnone {
                attrs.alwaysinline = true;
            }
            function.set_callee_attrs(func, attrs);
            let InstData::CallIndirect { args, .. } = function.inst_data(inst) else {
                unreachable!("candidate is still an indirect call");
            };
            function.replace_inst(
                inst,
                InstData::Call {
                    func,
                    args: args.clone(),
                },
            );
            changed = true;
        }
    }
    changed
}

fn def(function: &Function, value: Value) -> Option<&InstData> {
    let ValueDef::Result(inst, _) = function.value_def(function.resolve(value)) else {
        return None;
    };
    Some(function.inst_data(inst))
}
