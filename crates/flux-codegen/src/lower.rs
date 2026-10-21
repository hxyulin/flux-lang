//! MIR to keel IR.
//!
//! keel has no aggregate values, so a tuple or struct is split into its
//! scalar *leaves*, in field order, each a keel variable of its own: a
//! local of type `(i32, (bool, u64))` is three variables, and a field is a
//! range of its local's leaves. A local that is borrowed needs an address
//! instead, so it lives in a stack slot, laid out as a C struct of its
//! fields; so does whatever a pointer points to. Aggregates cross calls as one argument per
//! leaf; a function returning two or more leaves returns them through
//! memory the caller provides (`sret`), laid out as a C struct of the
//! leaves, which keel returns in registers where the platform allows.

use std::collections::HashMap;

use flux_hir::{FileHir, FunctionId};
use flux_mir::{
    BinOp, Body, Constant, FileMir, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind, UnOp, projection_ty,
};
use flux_ty::{FnSig, IntTy, Signatures, Ty, TyKind, TypedFile};
use keel_frontend::{FunctionBuilder, Variable};
use keel_ir::StackSlot;
use keel_ir::{
    AbiParam, ArgExt, Block, BlockCall, DataInit, DataItem, DataObject, FuncDecl, FuncRef,
    Function, FunctionAttrs, IntCC, Layout, MemoryFlags, Module, Purpose, Signature, Type, Value,
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
                // An associated function's path includes its type:
                // `crate::S::new`.
                let owner = f.owner.and_then(|i| hir.items.impls[i].strukt);
                let mut path = vec![crate_name];
                if let Some(strukt) = owner {
                    path.push(hir.items.structs[strukt].name.as_str());
                }
                path.push(f.name.as_str());
                mangle(&path)
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
                sig: signature(&typed.sigs, &typed.sigs[id], function.is_variadic),
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
    let main = hir.items.functions.iter().find(|(id, f)| {
        f.name.as_str() == "main"
            && !f.is_extern
            && f.owner.is_none()
            && mir.bodies.get(*id).is_some()
    });
    if let Some((id, _)) = main {
        module
            .functions
            .push(entry_point(&symbols[&id], &typed.sigs, &typed.sigs[id]));
    }
    module.sync_function_attrs();
    module
}

/// A legacy-style mangled name, `_ZN<len>crate<len>nameE` (with the
/// type's name between for an associated function), which
/// demanglers show as `crate::name`.
fn mangle(path: &[&str]) -> String {
    let mut symbol = "_ZN".to_owned();
    for segment in path {
        symbol.push_str(&format!("{}{segment}", segment.len()));
    }
    symbol.push('E');
    symbol
}

/// The keel type of a scalar: an integer, `bool` or pointer.
fn keel_type(ty: Ty) -> Option<Type> {
    match ty.kind() {
        TyKind::Int(int) => Some(int_type(int)),
        TyKind::Bool => Some(Type::I8),
        // `&CStr` is a thin pointer here: flux needs only `as_ptr`.
        TyKind::RawPtr(..) | TyKind::Ref(..) => Some(Type::Ptr),
        TyKind::Tuple(_)
        | TyKind::Adt(_)
        | TyKind::Never
        | TyKind::CStr
        | TyKind::Infer(_)
        | TyKind::Error => None,
    }
}

/// The scalar leaves of a value of `ty`, in field order: none for `()`
/// and `!`, one for a scalar.
fn leaves(sigs: &Signatures, ty: Ty, out: &mut Vec<Ty>) {
    match sigs.fields(ty) {
        Some(fields) => {
            for &field in fields {
                leaves(sigs, field, out);
            }
        }
        None if keel_type(ty).is_some() => out.push(ty),
        None => {}
    }
}

fn leaves_of(sigs: &Signatures, ty: Ty) -> Vec<Ty> {
    let mut out = Vec::new();
    leaves(sigs, ty, &mut out);
    out
}

fn leaf_count(sigs: &Signatures, ty: Ty) -> usize {
    match sigs.fields(ty) {
        Some(fields) => fields.iter().map(|&f| leaf_count(sigs, f)).sum(),
        None => usize::from(keel_type(ty).is_some()),
    }
}

/// How a value of `ty` is laid out in memory: a C struct of its fields,
/// recursively, with `()` and `!` empty.
fn layout(sigs: &Signatures, ty: Ty) -> Layout {
    match sigs.fields(ty) {
        Some(fields) => Layout::Struct(fields.iter().map(|&f| layout(sigs, f)).collect()),
        None => match keel_type(ty) {
            Some(scalar) => Layout::Scalar(scalar),
            None => Layout::Struct(Vec::new()),
        },
    }
}

/// The offset of field `index` in a value of `ty`, as [`layout`] places
/// it.
fn field_offset(sigs: &Signatures, ty: Ty, index: u32) -> u64 {
    let fields = sigs.fields(ty).expect("fields are of aggregates");
    let mut offset: u64 = 0;
    for (i, &field) in fields.iter().enumerate() {
        let field = layout(sigs, field);
        offset = offset.next_multiple_of(field.align());
        if i == index as usize {
            break;
        }
        offset += field.size();
    }
    offset
}

/// Where a place's value is.
#[derive(Clone, Copy)]
enum Location {
    /// Leaves `start..start + len` of a local held in variables.
    Vars {
        local: Local,
        start: usize,
        len: usize,
    },
    /// In memory, at an address.
    Memory(Value),
}

/// How a function returning `ret` returns it: nothing, one value, or
/// through memory with this layout.
enum Return {
    Values(Vec<AbiParam>),
    Sret(Layout),
}

fn return_kind(sigs: &Signatures, ret: Ty) -> Return {
    let leaves = leaves_of(sigs, ret);
    if leaves.len() <= 1 {
        return Return::Values(leaves.into_iter().filter_map(abi_param).collect());
    }
    let fields = leaves
        .iter()
        .map(|&leaf| Layout::Scalar(keel_type(leaf).expect("leaves are scalars")))
        .collect();
    Return::Sret(Layout::Struct(fields))
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

fn signature(sigs: &Signatures, sig: &FnSig, variadic: bool) -> Signature {
    let mut signature = Signature::new(&[], &[]);
    match return_kind(sigs, sig.ret) {
        Return::Values(returns) => signature.returns = returns,
        Return::Sret(layout) => signature.params.push(AbiParam {
            purpose: Purpose::Sret(layout),
            ..AbiParam::new(Type::Ptr)
        }),
    }
    for &param in &sig.params {
        let params = leaves_of(sigs, param).into_iter().filter_map(abi_param);
        signature.params.extend(params);
    }
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
fn entry_point(rust_main: &str, sigs: &Signatures, sig: &FnSig) -> Function {
    let mut function = Function::new(
        "main",
        Signature::new(&[Type::I32, Type::Ptr], &[Type::I32]),
    );
    let mut b = FunctionBuilder::new(&mut function);
    let entry = b.create_block();
    b.switch_to_block(entry);
    b.append_block_param(entry, Type::I32);
    b.append_block_param(entry, Type::Ptr);
    let callee = b.import_function(rust_main, signature(sigs, sig, false));
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
        let mut function = Function::new(
            self.symbols[&id].clone(),
            signature(&self.typed.sigs, sig, false),
        );
        function.attrs = attrs(sig);
        let typed = self.typed;
        let lower = Lower {
            ctx: self,
            sigs: &typed.sigs,
            body,
            b: FunctionBuilder::new(&mut function),
            blocks: Vec::new(),
            vars: Vec::new(),
            slots: Vec::new(),
            sret: None,
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
    sigs: &'a Signatures,
    body: &'c Body,
    b: FunctionBuilder<'f>,
    /// The keel block for each MIR block.
    blocks: Vec<Block>,
    /// The keel variables of each MIR local held in variables, one per
    /// leaf.
    vars: Vec<Box<[Variable]>>,
    /// The stack slot of each local that is borrowed.
    slots: Vec<Option<StackSlot>>,
    /// Where to store the return value, and its layout, when it is
    /// returned through memory.
    sret: Option<(Value, Layout)>,
    callees: HashMap<FunctionId, FuncRef>,
    /// Calls `abort`; shared by every failed assertion.
    panic_block: Option<Block>,
}

impl Lower<'_, '_, '_> {
    fn body(mut self) {
        let body = self.body;
        let sigs = self.sigs;
        self.blocks = body.blocks.iter().map(|_| self.b.create_block()).collect();
        // Locals whose own storage is borrowed need an address.
        let mut borrowed = vec![false; body.locals.len()];
        for data in body.blocks.values() {
            for statement in &data.statements {
                if let StatementKind::Assign(_, Rvalue::Ref { place, .. }) = &statement.kind
                    && !place.has_deref()
                {
                    borrowed[place.local.index()] = true;
                }
            }
        }
        for (local, decl) in body.locals.iter() {
            if borrowed[local.index()] {
                let layout = layout(sigs, decl.ty);
                let slot = self
                    .b
                    .create_stack_slot(layout.size().max(1) as u32, layout.align() as u32);
                self.slots.push(Some(slot));
                self.vars.push(Box::default());
            } else {
                let vars = leaves_of(sigs, decl.ty)
                    .into_iter()
                    .map(|leaf| {
                        self.b
                            .declare_var(keel_type(leaf).expect("leaves are scalars"))
                    })
                    .collect();
                self.slots.push(None);
                self.vars.push(vars);
            }
        }
        let entry = self.blocks[0];
        self.b.switch_to_block(entry);
        if let Return::Sret(layout) = return_kind(sigs, body.locals[Body::RETURN].ty) {
            let pointer = self.b.append_block_param(entry, Type::Ptr);
            self.sret = Some((pointer, layout));
        }
        for param in body.params() {
            let leaves = leaves_of(sigs, body.locals[param].ty);
            let values: Vec<Value> = leaves
                .into_iter()
                .map(|leaf| {
                    let ty = keel_type(leaf).expect("leaves are scalars");
                    self.b.append_block_param(entry, ty)
                })
                .collect();
            self.write(&param.into(), &values);
        }
        for (block, data) in body.blocks.iter() {
            self.b.switch_to_block(self.blocks[block.index()]);
            for statement in &data.statements {
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    let values = self.rvalue(rvalue);
                    self.write(dest, &values);
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

    /// Where `place`'s value is, and its type. Going through a pointer
    /// reads the pointer.
    fn locate(&mut self, place: &Place) -> (Location, Ty) {
        let sigs = self.sigs;
        let mut ty = self.body.locals[place.local].ty;
        let mut location = match self.slots[place.local.index()] {
            Some(slot) => Location::Memory(self.b.ins().stack_addr(slot)),
            None => Location::Vars {
                local: place.local,
                start: 0,
                len: self.vars[place.local.index()].len(),
            },
        };
        for &elem in &place.projection {
            let elem_ty = projection_ty(ty, elem, sigs);
            location = match (elem, location) {
                (ProjectionElem::Field(field), Location::Vars { local, start, .. }) => {
                    let fields = sigs.fields(ty).expect("fields are of aggregates");
                    let before: usize = fields[..field as usize]
                        .iter()
                        .map(|&f| leaf_count(sigs, f))
                        .sum();
                    Location::Vars {
                        local,
                        start: start + before,
                        len: leaf_count(sigs, elem_ty),
                    }
                }
                (ProjectionElem::Field(field), Location::Memory(address)) => {
                    let offset = field_offset(sigs, ty, field);
                    Location::Memory(self.offset(address, offset))
                }
                (ProjectionElem::Deref, location) => {
                    let pointer = self.load(location, ty)[0];
                    Location::Memory(pointer)
                }
            };
            ty = elem_ty;
        }
        (location, ty)
    }

    /// The leaves of the `ty` at `location`.
    fn load(&mut self, location: Location, ty: Ty) -> Vec<Value> {
        match location {
            Location::Vars { local, start, len } => {
                let vars = self.vars[local.index()][start..start + len].to_vec();
                vars.into_iter().map(|var| self.b.use_var(var)).collect()
            }
            Location::Memory(address) => self.load_leaves(address, &layout(self.sigs, ty)),
        }
    }

    fn read(&mut self, place: &Place) -> Vec<Value> {
        let (location, ty) = self.locate(place);
        self.load(location, ty)
    }

    fn write(&mut self, place: &Place, values: &[Value]) {
        let (location, ty) = self.locate(place);
        match location {
            Location::Vars { local, start, len } => {
                let vars = self.vars[local.index()][start..start + len].to_vec();
                for (var, &value) in vars.into_iter().zip(values) {
                    self.b.def_var(var, value);
                }
            }
            Location::Memory(address) => {
                let leaves = layout(self.sigs, ty).leaves();
                for ((offset, ty), &value) in leaves.into_iter().zip(values) {
                    let address = self.offset(address, offset);
                    self.b.ins().store_with_flags(value, address, aligned(ty));
                }
            }
        }
    }

    /// An operand's leaves.
    fn operand(&mut self, operand: &Operand) -> Vec<Value> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read(place),
            Operand::Const(constant) => self.constant(*constant).into_iter().collect(),
        }
    }

    /// A scalar operand's value; `None` for `()`.
    fn scalar(&mut self, operand: &Operand) -> Option<Value> {
        self.operand(operand).first().copied()
    }

    fn constant(&mut self, constant: Constant) -> Option<Value> {
        match constant {
            Constant::Int { bits, ty } => Some(self.int_const(bits, ty)),
            Constant::Bool(value) => Some(self.b.ins().iconst(Type::I8, i64::from(value))),
            Constant::Unit => None,
            Constant::CStr(index) => {
                let name = self.ctx.c_string(&self.body.c_strings[index as usize]);
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

    /// An rvalue's leaves.
    fn rvalue(&mut self, rvalue: &Rvalue) -> Vec<Value> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::Aggregate(_, operands) => {
                operands.iter().flat_map(|o| self.operand(o)).collect()
            }
            Rvalue::Ref { place, .. } => match self.locate(place).0 {
                Location::Memory(address) => vec![address],
                Location::Vars { .. } => unreachable!("borrowed locals are in memory"),
            },
            _ => self.scalar_rvalue(rvalue).into_iter().collect(),
        }
    }

    fn scalar_rvalue(&mut self, rvalue: &Rvalue) -> Option<Value> {
        match rvalue {
            Rvalue::Use(_) | Rvalue::Aggregate(..) | Rvalue::Ref { .. } => {
                unreachable!("not scalar operations")
            }
            Rvalue::UnaryOp(op, operand) => {
                let ty = self.body.operand_ty(operand, self.sigs);
                let value = self.scalar(operand)?;
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
                let from = self.body.operand_ty(operand, self.sigs);
                let value = self.scalar(operand)?;
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
        let ty = self.body.operand_ty(l, self.sigs);
        let signed = ty.is_signed_int();
        if ty == Ty::UNIT {
            // `() == ()`: the only value of `()` equals itself.
            let equal = matches!(op, BinOp::Eq | BinOp::Le | BinOp::Ge);
            return Some(self.b.ins().iconst(Type::I8, i64::from(equal)));
        }
        let a = self.scalar(l)?;
        let mut b = self.scalar(r)?;
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
                let amount_ty = self.body.operand_ty(r, self.sigs);
                let (amount_bits, value_bits) = (bits(amount_ty), bits(ty));
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
                let Some(value) = self.scalar(discr) else {
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
                let ret = self.ctx.typed.sigs[*func].ret;
                let sret = match return_kind(self.sigs, ret) {
                    Return::Sret(layout) => {
                        let slot = self
                            .b
                            .create_stack_slot(layout.size() as u32, layout.align() as u32);
                        Some((self.b.ins().stack_addr(slot), layout))
                    }
                    Return::Values(_) => None,
                };
                let mut values: Vec<Value> = sret.iter().map(|(pointer, _)| *pointer).collect();
                for arg in args {
                    values.extend(self.operand(arg));
                }
                let call = self.b.ins().call(callee, &values);
                let results = match &sret {
                    Some((pointer, layout)) => self.load_leaves(*pointer, layout),
                    None => self.b.function().inst_results(call).to_vec(),
                };
                self.write(dest, &results);
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
                let Some(cond) = self.scalar(cond) else {
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
                let values = self.read(&Body::RETURN.into());
                match self.sret.clone() {
                    Some((pointer, layout)) => {
                        for ((offset, ty), value) in layout.leaves().into_iter().zip(values) {
                            let address = self.offset(pointer, offset);
                            self.b.ins().store_with_flags(value, address, aligned(ty));
                        }
                        self.b.ins().return_(&[]);
                    }
                    None => {
                        self.b.ins().return_(&values);
                    }
                }
            }
            TerminatorKind::Unreachable => {
                self.b.ins().trap();
            }
        }
    }

    /// The leaves of a value laid out as `layout` at `pointer`.
    fn load_leaves(&mut self, pointer: Value, layout: &Layout) -> Vec<Value> {
        layout
            .leaves()
            .into_iter()
            .map(|(offset, ty)| {
                let address = self.offset(pointer, offset);
                self.b.ins().load_with_flags(ty, address, aligned(ty))
            })
            .collect()
    }

    fn offset(&mut self, pointer: Value, offset: u64) -> Value {
        if offset == 0 {
            return pointer;
        }
        let offset = self.b.ins().iconst(Type::I64, offset as i64);
        self.b.ins().ptradd(pointer, offset)
    }

    fn callee(&mut self, func: FunctionId) -> FuncRef {
        if let Some(&callee) = self.callees.get(&func) {
            return callee;
        }
        let variadic = self.ctx.hir.items.functions[func].is_variadic;
        let sig = signature(self.sigs, &self.ctx.typed.sigs[func], variadic);
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

/// Flags for an access to a `ty` in memory laid out by a [`Layout`],
/// which places each scalar at its natural alignment, its size.
fn aligned(ty: Type) -> MemoryFlags {
    MemoryFlags {
        align: Layout::Scalar(ty).align() as u32,
        ..MemoryFlags::default()
    }
}
