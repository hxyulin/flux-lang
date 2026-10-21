//! MIR to keel IR.

use std::collections::HashMap;

use flux_hir::{FileHir, FunctionId};
use flux_mir::{
    BinOp, Body, Constant, FileMir, Local, Operand, Rvalue, StatementKind, TerminatorKind, UnOp,
};
use flux_ty::{FnSig, IntTy, Ty, TyKind, TypedFile};
use keel_frontend::{FunctionBuilder, Variable};
use keel_ir::{
    AbiParam, ArgExt, Block, BlockCall, DataInit, DataItem, DataObject, FuncDecl, FuncRef,
    Function, FunctionAttrs, IntCC, Module, Signature, Type, Value,
};

/// The keel module for a type-checked file and its MIR. `crate_name`
/// prefixes the Rust functions' symbols.
pub fn lower_module(hir: &FileHir, typed: &TypedFile, mir: &FileMir, crate_name: &str) -> Module {
    let mut module = Module::default();
    let symbols: HashMap<FunctionId, String> = hir
        .items
        .functions
        .iter()
        .map(|(id, f)| {
            let name = if f.is_extern {
                f.name.to_string()
            } else {
                mangle(crate_name, f.name.as_str())
            };
            (id, name)
        })
        .collect();
    let mut declared_abort = false;
    for (id, function) in hir.items.functions.iter() {
        if function.is_extern {
            declared_abort |= function.name.as_str() == "abort";
            module.decls.push(FuncDecl {
                attrs: attrs(&typed.sigs[id]),
                name: symbols[&id].clone(),
                sig: signature(&typed.sigs[id], function.is_variadic),
            });
        }
    }
    let mut ctx = Ctx {
        hir,
        typed,
        symbols: &symbols,
        uses_abort: false,
        strings: HashMap::new(),
        data: Vec::new(),
    };
    for (id, body) in mir.bodies.iter() {
        module.functions.push(ctx.function(id, body));
    }
    module.data = ctx.data;
    if ctx.uses_abort && !declared_abort {
        module.decls.push(FuncDecl {
            attrs: FunctionAttrs {
                noreturn: true,
                nounwind: true,
                ..FunctionAttrs::default()
            },
            name: "abort".to_owned(),
            sig: Signature::new(&[], &[]),
        });
    }
    let main =
        hir.items.functions.iter().find(|(id, f)| {
            f.name.as_str() == "main" && !f.is_extern && mir.bodies.get(*id).is_some()
        });
    if let Some((id, _)) = main {
        module
            .functions
            .push(entry_point(&symbols[&id], &typed.sigs[id]));
    }
    module.sync_function_attrs();
    module
}

/// A legacy-style mangled name, `_ZN<len>crate<len>nameE`, which
/// demanglers show as `crate::name`.
fn mangle(crate_name: &str, name: &str) -> String {
    format!("_ZN{}{crate_name}{}{name}E", crate_name.len(), name.len())
}

/// The keel type of a value of `ty`; `None` for types without a value.
fn keel_type(ty: Ty) -> Option<Type> {
    match ty.kind() {
        TyKind::Int(int) => Some(int_type(int)),
        TyKind::Bool => Some(Type::I8),
        // `&CStr` is a thin pointer here: flux needs only `as_ptr`.
        TyKind::RawPtr(..) | TyKind::Ref(..) => Some(Type::Ptr),
        TyKind::Unit | TyKind::Never | TyKind::CStr | TyKind::Infer(_) | TyKind::Error => None,
    }
}

fn int_type(int: IntTy) -> Type {
    match int.bits() {
        8 => Type::I8,
        16 => Type::I16,
        32 => Type::I32,
        64 => Type::I64,
        _ => Type::I128,
    }
}

/// The width of an integer or `bool` value.
fn bits(ty: Ty) -> u32 {
    ty.as_int().map_or(8, IntTy::bits)
}

/// How a value of `ty` crosses a call: narrow integers are extended by
/// their signedness, as Apple's arm64 ABI requires of C callers.
fn abi_param(ty: Ty) -> Option<AbiParam> {
    let mut param = AbiParam::new(keel_type(ty)?);
    if matches!(param.ty, Type::I8 | Type::I16) {
        param.ext = if ty.is_signed_int() {
            ArgExt::Sext
        } else {
            ArgExt::Uext
        };
    }
    Some(param)
}

fn signature(sig: &FnSig, variadic: bool) -> Signature {
    let mut signature = Signature::new(&[], &[]);
    signature.params = sig.params.iter().filter_map(|&ty| abi_param(ty)).collect();
    signature.returns = abi_param(sig.ret).into_iter().collect();
    signature.variadic = variadic;
    signature
}

fn attrs(sig: &FnSig) -> FunctionAttrs {
    FunctionAttrs {
        noreturn: sig.ret == Ty::NEVER,
        // Flux has no unwinding: a panic aborts.
        nounwind: true,
        ..FunctionAttrs::default()
    }
}

/// `int main(int argc, char **argv)`: calls the Rust `main` and returns 0.
fn entry_point(rust_main: &str, sig: &FnSig) -> Function {
    let mut function = Function::new(
        "main",
        Signature::new(&[Type::I32, Type::Ptr], &[Type::I32]),
    );
    let mut b = FunctionBuilder::new(&mut function);
    let entry = b.create_block();
    b.switch_to_block(entry);
    b.append_block_param(entry, Type::I32);
    b.append_block_param(entry, Type::Ptr);
    let callee = b.import_function(rust_main, signature(sig, false));
    b.ins().call(callee, &[]);
    if sig.ret == Ty::NEVER {
        b.ins().trap();
    } else {
        let zero = b.ins().iconst(Type::I32, 0);
        b.ins().return_(&[zero]);
    }
    b.seal_all_blocks();
    b.finalize();
    function
}

struct Ctx<'a> {
    hir: &'a FileHir,
    typed: &'a TypedFile,
    symbols: &'a HashMap<FunctionId, String>,
    uses_abort: bool,
    /// The data object holding each distinct C string, by its bytes.
    strings: HashMap<Box<[u8]>, String>,
    data: Vec<DataObject>,
}

impl Ctx<'_> {
    fn function(&mut self, id: FunctionId, body: &Body) -> Function {
        let sig = &self.typed.sigs[id];
        let mut function = Function::new(self.symbols[&id].clone(), signature(sig, false));
        function.attrs = attrs(sig);
        let lower = Lower {
            ctx: self,
            body,
            b: FunctionBuilder::new(&mut function),
            blocks: Vec::new(),
            vars: Vec::new(),
            callees: HashMap::new(),
            panic_block: None,
        };
        lower.body();
        function
    }

    /// The name of the read-only data object holding `bytes` and a
    /// terminating NUL, shared by identical strings.
    fn c_string(&mut self, bytes: &[u8]) -> String {
        if let Some(name) = self.strings.get(bytes) {
            return name.clone();
        }
        let name = format!("__flux_str{}", self.data.len());
        let mut contents = bytes.to_vec();
        contents.push(0);
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            init: Some(DataInit {
                constant: true,
                align: 1,
                items: vec![DataItem::Bytes(contents)],
            }),
        });
        self.strings.insert(bytes.into(), name.clone());
        name
    }
}

struct Lower<'a, 'c, 'f> {
    ctx: &'c mut Ctx<'a>,
    body: &'c Body,
    b: FunctionBuilder<'f>,
    /// The keel block for each MIR block.
    blocks: Vec<Block>,
    /// The keel variable for each MIR local with a value.
    vars: Vec<Option<Variable>>,
    callees: HashMap<FunctionId, FuncRef>,
    /// Calls `abort`; shared by every failed assertion.
    panic_block: Option<Block>,
}

impl Lower<'_, '_, '_> {
    fn body(mut self) {
        let body = self.body;
        self.blocks = body.blocks.iter().map(|_| self.b.create_block()).collect();
        self.vars = body
            .locals
            .values()
            .map(|decl| keel_type(decl.ty).map(|ty| self.b.declare_var(ty)))
            .collect();
        let entry = self.blocks[0];
        self.b.switch_to_block(entry);
        for param in body.params() {
            if let (Some(var), Some(ty)) =
                (self.vars[param.index()], keel_type(body.locals[param].ty))
            {
                let value = self.b.append_block_param(entry, ty);
                self.b.def_var(var, value);
            }
        }
        for (block, data) in body.blocks.iter() {
            self.b.switch_to_block(self.blocks[block.index()]);
            for statement in &data.statements {
                let StatementKind::Assign(dest, rvalue) = &statement.kind;
                if let Some(value) = self.rvalue(rvalue)
                    && let Some(var) = self.vars[dest.index()]
                {
                    self.b.def_var(var, value);
                }
            }
            self.terminator(&data.terminator.kind);
        }
        if let Some(panic) = self.panic_block {
            self.b.switch_to_block(panic);
            let abort = self.b.import_function("abort", Signature::new(&[], &[]));
            self.b.ins().call(abort, &[]);
            self.b.ins().trap();
        }
        self.b.seal_all_blocks();
        self.b.finalize();
    }

    fn local(&mut self, local: Local) -> Option<Value> {
        let var = self.vars[local.index()]?;
        Some(self.b.use_var(var))
    }

    fn operand(&mut self, operand: &Operand) -> Option<Value> {
        match operand {
            Operand::Copy(local) => self.local(*local),
            Operand::Const(Constant::Int { bits, ty }) => Some(self.int_const(*bits, *ty)),
            Operand::Const(Constant::Bool(value)) => {
                Some(self.b.ins().iconst(Type::I8, i64::from(*value)))
            }
            Operand::Const(Constant::Unit) => None,
            Operand::Const(Constant::CStr(index)) => {
                let name = self.ctx.c_string(&self.body.c_strings[*index as usize]);
                let data = self.b.import_data(&name);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)))
            }
        }
    }

    /// An integer constant from its bits; 128-bit values that do not fit
    /// a sign-extended `i64` immediate are built from two halves.
    fn int_const(&mut self, bits: u128, int: IntTy) -> Value {
        let ty = int_type(int);
        let as_signed = if int.bits() < 128 {
            bits as i64
        } else {
            match i64::try_from(bits as i128) {
                Ok(value) => value,
                Err(_) => {
                    let low = self.b.ins().iconst(Type::I64, bits as u64 as i64);
                    let low = self.b.ins().uext(Type::I128, low);
                    let high = self.b.ins().iconst(Type::I128, (bits >> 64) as u64 as i64);
                    let shift = self.b.ins().iconst(Type::I128, 64);
                    let high = self.b.ins().shl(high, shift);
                    return self.b.ins().or(high, low);
                }
            }
        };
        self.b.ins().iconst(ty, as_signed)
    }

    fn rvalue(&mut self, rvalue: &Rvalue) -> Option<Value> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::UnaryOp(op, operand) => {
                let ty = self.body.operand_ty(operand);
                let value = self.operand(operand)?;
                Some(match op {
                    UnOp::Neg => self.b.ins().ineg(value),
                    UnOp::Not if ty == Ty::BOOL => {
                        let one = self.b.ins().iconst(Type::I8, 1);
                        self.b.ins().xor(value, one)
                    }
                    UnOp::Not => self.b.ins().bnot(value),
                })
            }
            Rvalue::BinaryOp(op, l, r) => self.binary(*op, l, r),
            Rvalue::Cast(operand, to) => {
                let from = self.body.operand_ty(operand);
                let value = self.operand(operand)?;
                let to_ty = keel_type(*to)?;
                let (from_bits, to_bits) = (bits(from), bits(*to));
                Some(if from_bits == to_bits {
                    value
                } else if to_bits < from_bits {
                    self.b.ins().trunc(to_ty, value)
                } else if from.is_signed_int() {
                    self.b.ins().sext(to_ty, value)
                } else {
                    self.b.ins().uext(to_ty, value)
                })
            }
        }
    }

    fn binary(&mut self, op: BinOp, l: &Operand, r: &Operand) -> Option<Value> {
        let ty = self.body.operand_ty(l);
        let signed = ty.is_signed_int();
        if ty == Ty::UNIT {
            // `() == ()`: the only value of `()` equals itself.
            let equal = matches!(op, BinOp::Eq | BinOp::Le | BinOp::Ge);
            return Some(self.b.ins().iconst(Type::I8, i64::from(equal)));
        }
        let a = self.operand(l)?;
        let mut b = self.operand(r)?;
        let cmp = |signed_cc, unsigned_cc| if signed { signed_cc } else { unsigned_cc };
        Some(match op {
            BinOp::Add => self.b.ins().iadd(a, b),
            BinOp::Sub => self.b.ins().isub(a, b),
            BinOp::Mul => self.b.ins().imul(a, b),
            BinOp::Div if signed => self.b.ins().sdiv(a, b),
            BinOp::Div => self.b.ins().udiv(a, b),
            BinOp::Rem if signed => self.b.ins().srem(a, b),
            BinOp::Rem => self.b.ins().urem(a, b),
            BinOp::BitAnd => self.b.ins().and(a, b),
            BinOp::BitOr => self.b.ins().or(a, b),
            BinOp::BitXor => self.b.ins().xor(a, b),
            BinOp::Shl | BinOp::Shr => {
                // keel takes shift amounts modulo the width, which is
                // Rust's masking; the amount needs the shifted value's type.
                let (amount_bits, value_bits) = (bits(self.body.operand_ty(r)), bits(ty));
                let value_ty = keel_type(ty)?;
                if amount_bits > value_bits {
                    b = self.b.ins().trunc(value_ty, b);
                } else if amount_bits < value_bits {
                    b = self.b.ins().uext(value_ty, b);
                }
                match op {
                    BinOp::Shl => self.b.ins().shl(a, b),
                    _ if signed => self.b.ins().sshr(a, b),
                    _ => self.b.ins().ushr(a, b),
                }
            }
            BinOp::Eq => self.b.ins().icmp(IntCC::Eq, a, b),
            BinOp::Ne => self.b.ins().icmp(IntCC::Ne, a, b),
            BinOp::Lt => self.b.ins().icmp(cmp(IntCC::Slt, IntCC::Ult), a, b),
            BinOp::Le => self.b.ins().icmp(cmp(IntCC::Sle, IntCC::Ule), a, b),
            BinOp::Gt => self.b.ins().icmp(cmp(IntCC::Sgt, IntCC::Ugt), a, b),
            BinOp::Ge => self.b.ins().icmp(cmp(IntCC::Sge, IntCC::Uge), a, b),
        })
    }

    fn terminator(&mut self, kind: &TerminatorKind) {
        match kind {
            TerminatorKind::Goto(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
            } => {
                let Some(value) = self.operand(discr) else {
                    self.b.ins().jump(self.blocks[otherwise.index()], &[]);
                    return;
                };
                let otherwise = self.blocks[otherwise.index()];
                match targets.as_slice() {
                    [(0, zero)] => {
                        let zero = self.blocks[zero.index()];
                        self.b.ins().brif(value, otherwise, &[], zero, &[]);
                    }
                    _ => {
                        let cases = targets
                            .iter()
                            .map(|&(value, block)| {
                                let call = BlockCall {
                                    block: self.blocks[block.index()],
                                    args: Vec::new(),
                                };
                                (value as i64, call)
                            })
                            .collect();
                        let default = BlockCall {
                            block: otherwise,
                            args: Vec::new(),
                        };
                        self.b.ins().switch(value, default, cases);
                    }
                }
            }
            TerminatorKind::Call {
                func,
                args,
                dest,
                target,
            } => {
                let callee = self.callee(*func);
                let args: Vec<Value> = args.iter().filter_map(|a| self.operand(a)).collect();
                let call = self.b.ins().call(callee, &args);
                let result = self.b.function().inst_results(call).first().copied();
                if let (Some(result), Some(var)) = (result, self.vars[dest.index()]) {
                    self.b.def_var(var, result);
                }
                match target {
                    Some(target) => {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    }
                    None => {
                        self.b.ins().trap();
                    }
                }
            }
            TerminatorKind::Assert {
                cond,
                expected,
                target,
                ..
            } => {
                let target = self.blocks[target.index()];
                let panic = self.panic_block();
                let Some(cond) = self.operand(cond) else {
                    self.b.ins().jump(target, &[]);
                    return;
                };
                if *expected {
                    self.b.ins().brif(cond, target, &[], panic, &[]);
                } else {
                    self.b.ins().brif(cond, panic, &[], target, &[]);
                }
            }
            TerminatorKind::Return => {
                let value = self.local(Body::RETURN);
                self.b.ins().return_(value.as_slice());
            }
            TerminatorKind::Unreachable => {
                self.b.ins().trap();
            }
        }
    }

    fn callee(&mut self, func: FunctionId) -> FuncRef {
        if let Some(&callee) = self.callees.get(&func) {
            return callee;
        }
        let variadic = self.ctx.hir.items.functions[func].is_variadic;
        let sig = signature(&self.ctx.typed.sigs[func], variadic);
        let callee = self.b.import_function(&self.ctx.symbols[&func], sig);
        self.callees.insert(func, callee);
        callee
    }

    fn panic_block(&mut self) -> Block {
        if let Some(block) = self.panic_block {
            return block;
        }
        self.ctx.uses_abort = true;
        let block = self.b.create_block();
        self.panic_block = Some(block);
        block
    }
}
