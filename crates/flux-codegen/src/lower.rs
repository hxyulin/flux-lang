//! MIR to keel IR.
//!
//! keel has no aggregate values, so a tuple or struct is split into its
//! scalar *leaves*, in field order, each a keel variable of its own: a
//! local of type `(i32, (bool, u64))` is three variables, and a field is a
//! range of its local's leaves. A local that is borrowed needs an address
//! instead, so it lives in a stack slot, laid out as rustc lays its type
//! out ([`flux_mir::layout`]); so does whatever a pointer points to.
//! Aggregates cross calls as one argument per leaf; a function returning
//! two or more leaves returns them through memory the caller provides
//! (`sret`), laid out as a C struct of the leaves, which keel returns in
//! registers where the platform allows.
//!
//! An enum's leaves are its discriminant, of its `repr` type (`isize`
//! without one), then the fields of each of its variants in turn: only
//! the variant it holds has meaningful leaves. In memory, its variants
//! share their bytes, so storing one stores only the fields of the
//! variant its discriminant says (a `switch` where several variants have
//! fields), then its tag, which loading decodes: a direct tag is
//! extended, a niche (`Option<&T>`'s null) compared with the values it
//! gives other variants.
//!
//! A value that contains an array (a *memory* type) is indexed at run
//! time, so it always lives in memory: its locals are stack slots, and
//! its value is the address of one, whose bytes are copied where it is
//! assigned. It crosses calls as that address, which the callee copies,
//! and is returned through `sret` memory laid out as the value is. A
//! reference to a slice is two leaves, the address of its first element
//! and its length.

use std::sync::Arc;

use flux_arena::{SliceId, SlicePool};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use flux_hir::{Diagnostic, FunctionId, Hir, ImplTarget, Owner};
use flux_mir::layout::{Layouts, Shape, TagEncoding};
use flux_mir::mono::Instance;
use flux_mir::{
    AggregateKind, AssertMessage, BinOp, Body, Constant, FileMir, Local, Operand, Place, PlaceTy,
    ProjectionElem, Rvalue, StatementKind, TerminatorKind, UnOp,
};
use flux_ty::{FnSig, GenericArgs, IntTy, Mutability, Signatures, Ty, TyKind, TypedFile};
use keel_frontend::{FunctionBuilder, Variable};
use keel_ir::ConvertOp;
use keel_ir::{
    AbiParam, ArgExt, Block, BlockCall, CallConv, DataInit, DataItem, DataObject, FuncDecl,
    FuncRef, Function, FunctionAttrs, IntCC, Intrinsic, Layout, MemoryFlags, Module, Purpose,
    Signature, Type, Value,
};

// Scalar values and fat pointers stay on the stack; large aggregates spill.
type Values = SmallVec<[Value; 4]>;
type TypeLeaves = SmallVec<[Ty; 4]>;

/// The keel module for a type-checked program and its MIR, `core`
/// included: one function per instance of a function the program needs.
/// An error if a generic function needs instances of itself without end.
pub fn lower_module(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
) -> Result<Module, Vec<Diagnostic>> {
    let collected = flux_mir::mono::collect_all(hir, &typed.sigs, mir).map_err(|d| vec![d])?;
    let instances = collected.instances;
    let mut module = Module::default();
    let items = &hir.items;
    let symbols: FxHashMap<FunctionId, String> = items
        .functions
        .iter()
        .map(|(id, f)| {
            let name = if f.is_extern {
                f.name.to_string()
            } else {
                // The function's path: its crate, its modules, and its
                // type for an associated function, `krate::m::S::new`,
                // where `S` is in `m` wherever the `impl` block is.
                let target = items.impl_of(id).and_then(|i| items.impls[i].target);
                let module = match target {
                    Some(ImplTarget::Struct(s)) => items.structs[s].loc.module,
                    _ => f.loc.module,
                };
                let mut path = Vec::new();
                let mut m = Some(module);
                while let Some(id) = m {
                    path.push(items.modules[id].name.as_str());
                    m = items.modules[id].parent;
                }
                path.reverse();
                match (target, f.owner) {
                    (Some(ImplTarget::Struct(s)), _) => path.push(items.structs[s].name.as_str()),
                    (Some(ImplTarget::Primitive(name)), _) => path.push(name.as_str()),
                    (Some(ImplTarget::Object(t)), _) => path.push(items.traits[t].name.as_str()),
                    (None, Some(Owner::Trait(t))) => path.push(items.traits[t].name.as_str()),
                    (None, Some(Owner::Impl(imp))) => {
                        if let Some(t) = items.impls[imp].trait_ {
                            path.push(items.traits[t].name.as_str());
                        }
                    }
                    (None, None) => {}
                }
                path.push(f.name.as_str());
                // Source crates may share a library name, and an impl may
                // live in a different crate from its target type. Item IDs
                // disambiguate this whole-program compilation; they are not
                // a stable identity for future separately compiled artifacts.
                let identity;
                if id.index() >= items.core.functions as usize {
                    identity = format!("__flux_f{}", id.index());
                    path.push(&identity);
                }
                mangle(&path)
            };
            (id, name)
        })
        .collect();
    let mut declared_abort = false;
    let mut declared = FxHashSet::default();
    let layouts = Layouts::new(items, &typed.sigs, &mir.discriminants);
    for (id, function) in hir.items.functions.iter() {
        // The same C function may be declared in several modules.
        if function.is_extern && declared.insert(symbols[&id].clone()) {
            declared_abort |= function.name.as_str() == "abort";
            module.decls.push(FuncDecl {
                attrs: attrs(&typed.sigs[id]),
                name: symbols[&id].clone(),
                sig: signature(&typed.sigs, &layouts, &typed.sigs[id], function.is_variadic),
            });
        }
    }
    // Each static's symbol, from its path as a function's is.
    let statics: FxHashMap<flux_hir::ConstId, String> = items
        .consts
        .iter()
        .filter(|(_, c)| c.is_static.is_some())
        .map(|(id, c)| {
            let mut path = Vec::new();
            let mut m = Some(c.loc.module);
            while let Some(id) = m {
                path.push(items.modules[id].name.as_str());
                m = items.modules[id].parent;
            }
            path.reverse();
            path.push(c.name.map_or("_", |n| n.as_str()));
            let identity;
            if id.index() >= items.core.consts as usize {
                identity = format!("__flux_s{}", id.index());
                path.push(&identity);
            }
            (id, mangle(&path))
        })
        .collect();
    let names = |id: flux_hir::ConstId| items.const_name(id);
    let mut ctx = Ctx {
        hir,
        typed,
        const_values: &mir.const_values,
        discriminants: &mir.discriminants,
        evaluator: flux_mir::eval::Evaluator::new(
            items,
            &typed.sigs,
            &mir.bodies,
            &mir.consts,
            &names,
        ),
        instance_values: FxHashMap::default(),
        symbols: &symbols,
        statics: &statics,
        reified: collected.reified,
        uses_abort: false,
        locations: FxHashMap::default(),
        strings: FxHashMap::default(),
        vtables: FxHashMap::default(),
        promoted: FxHashMap::default(),
        data: Vec::new(),
        layouts,
    };
    for instance in instances {
        let body = flux_mir::mono::body_of(hir, &typed.sigs, mir, instance);
        flux_mir::validate::assert_valid(
            &hir.items,
            &typed.sigs,
            &[],
            &body,
            &hir.items.functions[instance.function].name,
        );
        let function = ctx.function(instance, &body);
        module.functions.push(function);
    }
    // Constants whose values depend on generic arguments fail only
    // once code needs them, as in rustc.
    if !ctx.evaluator.failures.is_empty() {
        let mut errors: Vec<Diagnostic> = ctx
            .evaluator
            .failures
            .iter()
            .map(|(c, e)| {
                let mut d = Diagnostic::new(e.range, e.message.clone());
                d.file = items.consts[c.konst].loc.file;
                d
            })
            .collect();
        Diagnostic::sort(&mut errors);
        errors.dedup();
        return Err(errors);
    }
    // Every static, with its initial value, in memory a `static mut`
    // may write.
    for (&id, symbol) in &statics {
        let Some(value) = mir.const_values.items.get(id) else {
            continue;
        };
        let ty = typed.sigs.consts[id];
        let (bytes, align) = ctx.value_bytes(value, ty);
        ctx.data.push(DataObject {
            name: symbol.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: !items.consts[id].is_static_mut(),
                align,
                items: bytes,
            }),
        });
    }
    ctx.data.sort_by(|a, b| a.name.cmp(&b.name));
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
    let root = items.module_of(items.main_crate());
    let main = items.functions.iter().find(|(id, f)| {
        f.name.as_str() == "main"
            && f.loc.module == root
            && !f.is_extern
            && f.owner.is_none()
            && mir.bodies.get(*id).is_some()
    });
    if let Some((id, _)) = main {
        let instance = Instance {
            function: id,
            args: &[],
        };
        let sig = &typed.sigs[id];
        let main_sig = ctx_signature(
            &typed.sigs,
            &ctx.layouts,
            sig,
            ctx.reified.contains(&instance),
        );
        let flush = items
            .lang_fns
            .get(&flux_intern::Symbol::intern("flush_stdout"))
            .filter(|&&f| mir.bodies.get(f).is_some())
            .map(|&f| {
                let instance = Instance {
                    function: f,
                    args: &[],
                };
                let flush_sig = ctx_signature(&typed.sigs, &ctx.layouts, &typed.sigs[f], false);
                (instance_symbol(hir, &symbols, instance), flush_sig)
            });
        let entry = entry_point(&symbols[&id], &main_sig, sig.ret == Ty::NEVER, flush);
        module.functions.push(entry);
    }
    module.sync_function_attrs();
    Ok(module)
}

/// The symbol of `instance`: its function's, with a hash as a last
/// segment, as rustc's legacy mangling does, if it has generic arguments
/// or is in one of a generic struct's blocks, which may each have a
/// function of its name. The hash covers the arguments and the block.
fn instance_symbol(
    hir: &Hir,
    symbols: &FxHashMap<FunctionId, String>,
    instance: Instance,
) -> String {
    let base = &symbols[&instance.function];
    let items = &hir.items;
    // A trait's blocks for different types each have its methods.
    let block = items.impl_of(instance.function).filter(|&owner| {
        let imp = &items.impls[owner];
        match imp.target {
            Some(ImplTarget::Struct(s)) => !items.structs[s].generics.is_empty(),
            _ => imp.trait_ref.is_some(),
        }
    });
    if instance.args.is_empty() && block.is_none() {
        return base.clone();
    }
    // FNV-1a, over a structural encoding of the arguments.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    };
    if let Some(block) = block {
        feed(&(block.index() as u32).to_le_bytes());
    }
    fn encode(ty: Ty, feed: &mut impl FnMut(&[u8])) {
        match ty.kind() {
            TyKind::Int(int) => feed(int.name().as_str().as_bytes()),
            TyKind::Bool => feed(b"b"),
            TyKind::Char => feed(b"c"),
            TyKind::Str => feed(b"str"),
            TyKind::Never => feed(b"!"),
            TyKind::CStr => feed(b"CStr"),
            TyKind::Tuple(elements) => {
                feed(b"(");
                elements.iter().for_each(|&e| encode(e, feed));
                feed(b")");
            }
            TyKind::Adt(adt, args) => {
                feed(b"S");
                feed(&(adt.strukt.index() as u32).to_le_bytes());
                args.iter().for_each(|&a| encode(a, feed));
                feed(b";");
            }
            TyKind::Array(elem, len) => {
                feed(b"[");
                encode(elem, feed);
                feed(&array_len(len).to_le_bytes());
                feed(b"]");
            }
            TyKind::Slice(elem) => {
                feed(b"[");
                encode(elem, feed);
                feed(b"]");
            }
            TyKind::Closure(id, args) => {
                feed(b"C");
                feed(&(id.owner.index() as u32).to_le_bytes());
                feed(&(id.expr.index() as u32).to_le_bytes());
                args.iter().for_each(|&a| encode(a, feed));
                feed(b";");
            }
            TyKind::RawPtr(m, pointee) | TyKind::Ref(m, pointee) => {
                let pointer: &[u8] = if matches!(ty.kind(), TyKind::Ref(..)) {
                    b"&"
                } else {
                    b"*"
                };
                feed(pointer);
                feed(if m == Mutability::Mut { b"m" } else { b"c" });
                encode(pointee, feed);
            }
            TyKind::FnPtr(sig) => {
                feed(if sig.is_unsafe { b"uF(" } else { b"F(" });
                sig.inputs_output.iter().for_each(|&t| encode(t, feed));
                feed(b")");
            }
            TyKind::FnDef(def) => {
                feed(b"D");
                feed(&(def.function.index() as u32).to_le_bytes());
                def.args.iter().for_each(|&a| encode(a, feed));
                feed(b";");
            }
            TyKind::Value(value) => {
                feed(b"V");
                encode(value.ty, feed);
                feed(&value.bits().to_le_bytes());
            }
            TyKind::Dynamic(d) => {
                feed(b"dyn");
                feed(&(d.trait_.index() as u32).to_le_bytes());
                d.args
                    .iter()
                    .chain(d.assoc_tys)
                    .for_each(|&a| encode(a, feed));
                feed(b";");
            }
            TyKind::Param(_)
            | TyKind::Alias(_)
            | TyKind::Opaque(_)
            | TyKind::Infer(_)
            | TyKind::Error => {
                unreachable!("instances are of concrete types")
            }
        }
    }
    for &arg in instance.args {
        encode(arg, &mut feed);
    }
    format!("{}17h{hash:016x}E", &base[..base.len() - 1])
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
        TyKind::Char => Some(Type::I32),
        // `&CStr` is a thin pointer here: flux needs only `as_ptr`.
        TyKind::RawPtr(..) | TyKind::Ref(..) | TyKind::FnPtr(_) => Some(Type::Ptr),
        // A function item has no value: it is its type.
        TyKind::FnDef(_) => None,
        TyKind::Tuple(_)
        | TyKind::Adt(..)
        | TyKind::Array(..)
        | TyKind::Slice(_)
        | TyKind::Str
        | TyKind::Dynamic(_)
        | TyKind::Closure(..)
        | TyKind::Param(_)
        | TyKind::Alias(_)
        | TyKind::Opaque(_)
        | TyKind::Never
        | TyKind::CStr
        | TyKind::Infer(_)
        | TyKind::Value(_)
        | TyKind::Error => None,
    }
}

/// The length of an array type, `len`: known once monomorphized.
fn array_len(len: Ty) -> u64 {
    len.as_usize()
        .expect("arrays' lengths are known once monomorphized")
}

/// The type of the discriminant of `ty`, an enum.
fn discriminant(sigs: &Signatures, ty: Ty) -> flux_ty::IntTy {
    sigs.discriminant_ty(ty).unwrap_or(flux_ty::IntTy::Isize)
}

/// The unsigned integer type of a tag `bytes` wide.
fn tag_int(bytes: u8) -> IntTy {
    match bytes {
        1 => IntTy::U8,
        2 => IntTy::U16,
        4 => IntTy::U32,
        8 => IntTy::U64,
        _ => IntTy::U128,
    }
}

/// Whether values of `ty` contain an array, so live in memory.
fn is_memory(sigs: &Signatures, ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Array(..) => true,
        TyKind::Adt(adt, _) if sigs.unions.contains(&adt.strukt) => true,
        TyKind::Tuple(_) | TyKind::Adt(..) | TyKind::Closure(..) => sigs
            .contents(ty)
            .is_some_and(|fields| fields.iter().any(|&f| is_memory(sigs, f))),
        _ => false,
    }
}

/// A pointer to a slice, `str` or trait object: its address, then its
/// length or vtable.
fn is_fat(ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if p.is_unsized())
}

/// A pointer to a trait object: its address, then its vtable's.
fn is_dyn_ptr(ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if p.is_dyn())
}

/// What a pointer or `Box` of type `ty` points to.
fn pointee_of(ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::Ref(_, p) | TyKind::RawPtr(_, p) => Some(p),
        _ => flux_ty::box_pointee(ty),
    }
}

/// A `Box` of a trait object.
fn is_dyn_box(ty: Ty) -> bool {
    flux_ty::box_pointee(ty).is_some_and(|p| p.is_dyn())
}

/// The second leaf of a fat pointer: a length, or a vtable's address.
fn metadata_leaf(ty: Ty) -> Ty {
    if is_dyn_ptr(ty) {
        address_leaf()
    } else {
        Ty::int(IntTy::Usize)
    }
}

/// The leaf of a value of a memory type: the address of its bytes.
fn address_leaf() -> Ty {
    Ty::raw_ptr(Mutability::Not, Ty::UNIT)
}

/// The scalar leaves of a value of `ty`, in field order: none for `()`
/// and `!`, one for a scalar; an enum's discriminant, then every
/// variant's fields; the address of a value of a memory type; the
/// address and length of a pointer to a slice.
fn leaves(sigs: &Signatures, ty: Ty, out: &mut TypeLeaves) {
    if is_memory(sigs, ty) {
        out.push(address_leaf());
        return;
    }
    if is_fat(ty) {
        out.push(address_leaf());
        out.push(metadata_leaf(ty));
        return;
    }
    if let Some(variants) = sigs.variants(ty) {
        out.push(Ty::int(discriminant(sigs, ty)));
        for fields in variants {
            for &field in fields {
                leaves(sigs, field, out);
            }
        }
        return;
    }
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

fn leaves_of(sigs: &Signatures, ty: Ty) -> TypeLeaves {
    let mut out = TypeLeaves::new();
    leaves(sigs, ty, &mut out);
    out
}

fn leaf_count(sigs: &Signatures, ty: Ty) -> usize {
    if is_memory(sigs, ty) {
        return 1;
    }
    if is_fat(ty) {
        return 2;
    }
    if let Some(variants) = sigs.variants(ty) {
        return 1 + variants
            .iter()
            .flat_map(|fields| fields.iter())
            .map(|&f| leaf_count(sigs, f))
            .sum::<usize>();
    }
    match sigs.fields(ty) {
        Some(fields) => fields.iter().map(|&f| leaf_count(sigs, f)).sum(),
        None => usize::from(keel_type(ty).is_some()),
    }
}

/// A keel layout of `size` bytes aligned to `align`, for an `sret`
/// pointer to a value of a memory type: keel needs only its bytes.
fn bytes_layout(size: u64, align: u64) -> Layout {
    for scalar in [Type::I64, Type::I32, Type::I16] {
        let bytes = Layout::Scalar(scalar).size();
        if align >= bytes && size.is_multiple_of(bytes) {
            return Layout::Array(Box::new(Layout::Scalar(scalar)), (size / bytes) as u32);
        }
    }
    Layout::Array(Box::new(Layout::Scalar(Type::I8)), size as u32)
}

/// The fields of a value of `place`'s type: of the variant it was
/// downcast to, or its own.
fn place_fields(sigs: &Signatures, place: PlaceTy) -> &'static [Ty] {
    sigs.variant_fields(place.ty, place.variant.unwrap_or(0))
        .expect("fields are of aggregates")
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
    /// A value of a memory type, laid out as it is.
    Memory(Layout),
}

fn return_kind(sigs: &Signatures, layouts: &Layouts, ret: Ty) -> Return {
    if is_memory(sigs, ret) {
        let layout = layouts.of(ret);
        // Nothing to return (`[T; 0]`): keel has no empty `sret`.
        if layout.size == 0 {
            return Return::Values(Vec::new());
        }
        return Return::Memory(bytes_layout(layout.size, layout.align));
    }
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

/// The width of an integer, `char` or `bool` value.
fn bits(ty: Ty) -> u32 {
    match ty.kind() {
        TyKind::Char => 32,
        _ => ty.as_int().map_or(8, IntTy::bits),
    }
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

fn signature(sigs: &Signatures, layouts: &Layouts, sig: &FnSig, variadic: bool) -> Signature {
    let mut signature = Signature::new(&[], &[]);
    match return_kind(sigs, layouts, sig.ret) {
        Return::Values(returns) => signature.returns = returns,
        Return::Sret(layout) | Return::Memory(layout) => signature.params.push(AbiParam {
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

/// The signature of a function flux defines: keel's private convention
/// (`cc(fast)`), which leaves narrow values unextended, where it allows
/// it; that is for one result at most, and every function flux defines is
/// local and called directly.
fn flux_signature(sigs: &Signatures, layouts: &Layouts, sig: &FnSig) -> Signature {
    let mut signature = signature(sigs, layouts, sig, false);
    if signature.returns.len() <= 1 {
        signature.cc = CallConv::Fast;
        for param in signature.params.iter_mut().chain(&mut signature.returns) {
            param.ext = ArgExt::None;
        }
    }
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

/// The signature of a function flux defines: keel's private convention
/// unless its address is taken (`reified`), when it must be called
/// through a pointer, which `cc(fast)` does not allow: C's then.
fn ctx_signature(sigs: &Signatures, layouts: &Layouts, sig: &FnSig, reified: bool) -> Signature {
    if reified {
        signature(sigs, layouts, sig, false)
    } else {
        flux_signature(sigs, layouts, sig)
    }
}

/// `int main(int argc, char **argv)`: calls the Rust `main`, then
/// `flush`, which writes out what stdout holds, as std's runtime does,
/// and returns 0.
fn entry_point(
    rust_main: &str,
    main_sig: &Signature,
    never: bool,
    flush: Option<(String, Signature)>,
) -> Function {
    let mut function = Function::new(
        "main",
        Signature::new(&[Type::I32, Type::Ptr], &[Type::I32]),
    );
    let mut b = FunctionBuilder::new(&mut function);
    let entry = b.create_block();
    b.switch_to_block(entry);
    b.append_block_param(entry, Type::I32);
    b.append_block_param(entry, Type::Ptr);
    let callee = b.import_function(rust_main, main_sig.clone());
    b.ins().call(callee, &[]);
    if let Some((flush, flush_sig)) = flush.filter(|_| !never) {
        let flush = b.import_function(&flush, flush_sig);
        b.ins().call(flush, &[]);
    }
    if never {
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
    hir: &'a Hir,
    typed: &'a TypedFile,
    /// The values of constants that code without generic parameters
    /// names.
    const_values: &'a flux_mir::eval::ConstValues,
    /// The discriminants of enums that are not literals'.
    discriminants: &'a flux_mir::Discriminants,
    /// Evaluates the others, for the instances that use them.
    evaluator: flux_mir::eval::Evaluator<'a>,
    instance_values: FxHashMap<flux_ty::ConstInstance, flux_mir::eval::Value>,
    symbols: &'a FxHashMap<FunctionId, String>,
    /// The data object of each static.
    statics: &'a FxHashMap<flux_hir::ConstId, String>,
    /// The instances whose address the program takes.
    reified: FxHashSet<Instance>,
    uses_abort: bool,
    /// The data object of each `Location` a `#[track_caller]` function
    /// is passed, by file and offset.
    locations: FxHashMap<(flux_hir::FileId, u32), String>,
    /// The data object holding each distinct C string, by its bytes.
    strings: FxHashMap<Box<[u8]>, String>,
    /// The vtable of each type as each trait object: `(object, type)`.
    vtables: FxHashMap<(Ty, Ty), String>,
    /// The data objects of promoted constants, by their bytes and
    /// alignment.
    promoted: FxHashMap<(Vec<u8>, u32), String>,
    data: Vec<DataObject>,
    /// How values are laid out in memory, as rustc lays them out.
    layouts: Layouts<'a>,
}

impl Ctx<'_> {
    /// The discriminant of variant `variant` of enum `strukt`, as bits.
    fn discriminant(&mut self, strukt: flux_hir::StructId, variant: u32) -> u128 {
        let items = &self.hir.items;
        flux_mir::discriminant_bits(items, self.discriminants, strukt, variant)
            .or_else(|| self.evaluator.discriminant(strukt, variant).ok())
            .expect("discriminants are evaluated before code generation")
    }

    /// `instance`, whose MIR is `body` with its generic arguments
    /// substituted.
    fn function(&mut self, instance: Instance, body: &Body) -> Function {
        let sig = &flux_ty::traits::instance_sig(
            &self.hir.items,
            &self.typed.sigs,
            instance.function,
            instance.args,
        );
        let takes_location = self.takes_location(instance);
        let mut signature = ctx_signature(
            &self.typed.sigs,
            &self.layouts,
            sig,
            self.reified.contains(&instance),
        );
        if takes_location {
            signature.params.push(AbiParam::new(Type::Ptr));
        }
        let mut function =
            Function::new(instance_symbol(self.hir, self.symbols, instance), signature);
        function.local = true;
        function.attrs = attrs(sig);
        let typed = self.typed;
        let owner = body.owner.unwrap_or(instance.function);
        let file = self.hir.items.functions[owner].loc.file;
        let lower = Lower {
            ctx: self,
            sigs: &typed.sigs,
            body,
            file,
            takes_location,
            caller_location: None,
            b: FunctionBuilder::new(&mut function),
            blocks: Vec::new(),
            vars: Vec::new(),
            variable_lists: SlicePool::default(),
            slots: Vec::new(),
            sret: None,
            callees: FxHashMap::default(),
            panic_block: None,
        };
        lower.body();
        function
    }

    /// Whether `instance` takes where it is called from, as a pointer
    /// to a `Location` after its parameters: a `#[track_caller]`
    /// function's, unless its address is taken, as a pointer's
    /// signature has no room for it.
    fn takes_location(&self, instance: Instance) -> bool {
        self.hir.items.functions[instance.function].track_caller
            && !self.reified.contains(&instance)
    }

    /// The data object of the `Location` of `offset` in `file`: its
    /// path as the compiler was given it, and its one-based line and
    /// column, in characters, as rustc counts them.
    fn location(&mut self, file: flux_hir::FileId, offset: u32) -> String {
        let items = &self.hir.items;
        let file = items.expansion_files.get(&file).copied().unwrap_or(file);
        if let Some(name) = self.locations.get(&(file, offset)) {
            return name.clone();
        }
        let (path, line, column) = match items.files.get(&file) {
            Some(source) => {
                let text = source.text();
                let mut at = (offset as usize).min(text.len());
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                let before = &text[..at];
                let line = before.matches('\n').count() + 1;
                let line_start = before.rfind('\n').map_or(0, |i| i + 1);
                let column = before[line_start..].chars().count() + 1;
                (source.path.display().to_string(), line, column)
            }
            None => (String::new(), 1, 1),
        };
        use flux_mir::eval::Value as V;
        let bytes = path
            .bytes()
            .map(|b| V::Int {
                bits: u128::from(b),
                ty: IntTy::U8,
            })
            .collect();
        let value = V::Aggregate(
            vec![
                V::Ref(Arc::new(V::Aggregate(bytes))),
                V::Int {
                    bits: line as u128,
                    ty: IntTy::U32,
                },
                V::Int {
                    bits: column as u128,
                    ty: IntTy::U32,
                },
            ]
            .into(),
        );
        let strukt = items.lang_enums[&flux_intern::Symbol::intern("Location")];
        let ty = Ty::adt(strukt, items.structs[strukt].name, &[]);
        let name = self.promoted(&value, ty);
        self.locations.insert((file, offset), name.clone());
        name
    }

    /// `value`, of type `ty`, laid out in memory: its bytes and
    /// alignment.
    fn value_bytes(&mut self, value: &flux_mir::eval::Value, ty: Ty) -> (Vec<DataItem>, u32) {
        let layout = self.layouts.of(ty);
        let mut bytes = vec![0u8; layout.size.max(1) as usize];
        // The addresses, which the linker fills in, by offset.
        let mut addresses: Vec<(usize, String)> = Vec::new();
        self.write_bytes(value, ty, 0, &mut bytes, &mut addresses);
        let mut items = Vec::new();
        let mut at = 0;
        addresses.sort();
        for (offset, symbol) in addresses {
            if offset > at {
                items.push(DataItem::Bytes(bytes[at..offset].to_vec()));
            }
            items.push(DataItem::Ptr { symbol, addend: 0 });
            at = offset + 8;
        }
        if at < bytes.len() {
            items.push(DataItem::Bytes(bytes[at..].to_vec()));
        }
        (items, layout.align as u32)
    }

    /// Writes `value`, of type `ty`, into `bytes` at `at`, laid out as
    /// in memory; the addresses it holds go in `addresses`.
    fn write_bytes(
        &mut self,
        value: &flux_mir::eval::Value,
        ty: Ty,
        at: usize,
        bytes: &mut [u8],
        addresses: &mut Vec<(usize, String)>,
    ) {
        use flux_mir::eval::Value as V;
        let put = |bytes: &mut [u8], at: usize, bits: u128, width: usize| {
            bytes[at..at + width].copy_from_slice(&bits.to_le_bytes()[..width]);
        };
        let layout = self.layouts.of(ty);
        match value {
            V::Int { bits, .. } => put(bytes, at, *bits, layout.size as usize),
            V::Bool(b) => bytes[at] = u8::from(*b),
            V::Static(id) => addresses.push((at, self.statics[id].clone())),
            V::Ref(pointee) => {
                let (symbol, len) = self.referenced(pointee, ty);
                addresses.push((at, symbol));
                if let Some(len) = len {
                    put(bytes, at + 8, u128::from(len), 8);
                }
            }
            V::Aggregate(elements) if matches!(ty.kind(), TyKind::Array(..)) => {
                let elem = ty.sequence_elem().unwrap_or(Ty::ERROR);
                let stride = self.layouts.of(elem).size as usize;
                for (i, element) in elements.iter().enumerate() {
                    self.write_bytes(element, elem, at + i * stride, bytes, addresses);
                }
            }
            V::Aggregate(fields) => {
                let types = self.typed.sigs.fields(ty).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let offset = layout.field_offset(0, i as u32) as usize;
                    self.write_bytes(field, field_ty, at + offset, bytes, addresses);
                }
            }
            V::Variant {
                strukt,
                index,
                fields,
            } => {
                let types = self
                    .typed
                    .sigs
                    .variant_fields(ty, *index)
                    .unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let offset = layout.field_offset(*index, i as u32) as usize;
                    self.write_bytes(field, field_ty, at + offset, bytes, addresses);
                }
                if let Some((offset, width, bits)) = self.tag_store(ty, *strukt, *index) {
                    put(bytes, at + offset as usize, bits, usize::from(width));
                }
            }
        }
    }

    /// What a value of variant `variant` of `ty`, an enum of `strukt`,
    /// has in its tag: its offset, width and bits; none where the tag
    /// is the variant's own fields (a niche-filling enum's untagged
    /// variant) or there is none.
    fn tag_store(
        &mut self,
        ty: Ty,
        strukt: flux_hir::StructId,
        variant: u32,
    ) -> Option<(u64, u8, u128)> {
        let layout = self.layouts.of(ty);
        let Shape::Enum { tag: Some(tag), .. } = &layout.shape else {
            return None;
        };
        let mask = if tag.bytes >= 16 {
            u128::MAX
        } else {
            (1u128 << (8 * u32::from(tag.bytes))) - 1
        };
        let bits = match tag.encoding {
            TagEncoding::Direct => self.discriminant(strukt, variant),
            TagEncoding::Niche { untagged, .. } if untagged == variant => return None,
            TagEncoding::Niche { first, start, .. } => {
                u128::from(variant.wrapping_sub(first)).wrapping_add(start)
            }
        };
        Some((tag.offset, tag.bytes, bits & mask))
    }

    /// The name of the read-only data object holding `value`, of type
    /// `ty`: a promoted constant, shared by identical ones.
    fn promoted(&mut self, value: &flux_mir::eval::Value, ty: Ty) -> String {
        let (items, align) = self.value_bytes(value, ty);
        let key = (format!("{items:?}").into_bytes(), align);
        if let Some(name) = self.promoted.get(&key) {
            return name.clone();
        }
        let name = format!("__flux_promoted{}", self.data.len());
        self.promoted.insert(key, name.clone());
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: true,
                align,
                items,
            }),
        });
        name
    }

    /// The data object a reference of type `ty` to `pointee`, in a
    /// constant, points to, and for a slice its length.
    fn referenced(&mut self, pointee: &flux_mir::eval::Value, ty: Ty) -> (String, Option<u64>) {
        let target = match ty.kind() {
            TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t,
            _ => Ty::ERROR,
        };
        match (target.kind(), pointee) {
            (TyKind::Slice(elem), flux_mir::eval::Value::Aggregate(elements)) => {
                let len = elements.len() as u64;
                let array = Ty::array(elem, len);
                (self.promoted(pointee, array), Some(len))
            }
            // A `str` is its UTF-8 bytes.
            (TyKind::Str, flux_mir::eval::Value::Aggregate(bytes)) => {
                let len = bytes.len() as u64;
                let array = Ty::array(Ty::int(IntTy::U8), len);
                (self.promoted(pointee, array), Some(len))
            }
            _ => (self.promoted(pointee, target), None),
        }
    }

    /// The name of the read-only data object of `concrete`'s vtable as
    /// trait object `object` ([`flux_mir::vtable`]): the function that
    /// drops a value of the type in place (if it needs one and `core`
    /// has `ptr::drop_in_place`), its size and alignment, then its
    /// slots.
    fn vtable(&mut self, object: Ty, concrete: Ty) -> String {
        if let Some(name) = self.vtables.get(&(object, concrete)) {
            return name.clone();
        }
        let name = format!("__flux_vtable{}", self.vtables.len());
        self.vtables.insert((object, concrete), name.clone());
        let sigs = &self.typed.sigs;
        let layout = self.layouts.of(concrete);
        let word = |value: u64| DataItem::Int {
            ty: Type::I64,
            value: value as i64,
        };
        let drop = match flux_mir::vtable::drop_entry(&self.hir.items, sigs, concrete) {
            Some(instance) => DataItem::Ptr {
                symbol: instance_symbol(self.hir, self.symbols, instance),
                addend: 0,
            },
            None => DataItem::Zero(8),
        };
        let mut items = vec![drop, word(layout.size), word(layout.align)];
        let entries = flux_mir::vtable::entries(&self.hir.items, sigs, object, concrete);
        for entry in entries {
            items.push(match entry {
                flux_mir::vtable::Entry::Null => DataItem::Zero(8),
                flux_mir::vtable::Entry::Method(instance) => DataItem::Ptr {
                    symbol: instance_symbol(self.hir, self.symbols, instance),
                    addend: 0,
                },
                flux_mir::vtable::Entry::Super(sup) => DataItem::Ptr {
                    symbol: self.vtable(sup, concrete),
                    addend: 0,
                },
            });
        }
        self.data.push(DataObject {
            name: name.clone(),
            thread_local: false,
            local: true,
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
            init: Some(DataInit {
                constant: true,
                align: 8,
                items,
            }),
        });
        name
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
            weak_odr: false,
            weak: false,
            section: None,
            retain: false,
            hidden: false,
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
    /// The file the body's ranges are in.
    file: flux_hir::FileId,
    /// Whether the function takes where it was called from.
    takes_location: bool,
    /// That parameter, a pointer to a `Location`.
    caller_location: Option<Value>,
    b: FunctionBuilder<'f>,
    /// The keel block for each MIR block.
    blocks: Vec<Block>,
    /// The keel variables of each MIR local held in variables, one per
    /// leaf.
    vars: Vec<SliceId<Variable>>,
    variable_lists: SlicePool<Variable>,
    /// The storage address of each local that is borrowed or lives in memory.
    slots: Vec<Option<Value>>,
    /// Where to store the return value, and its layout, when it is
    /// returned through memory.
    sret: Option<(Value, Layout)>,
    callees: FxHashMap<Instance, FuncRef>,
    /// Calls `abort`; shared by every failed assertion.
    panic_block: Option<Block>,
}

impl Lower<'_, '_, '_> {
    fn body(mut self) {
        let body = self.body;
        let sigs = self.sigs;
        // The parameters are stored where they live first, which may
        // take blocks of its own (an enum's variants), then `bb0` runs.
        let entry = self.b.create_block();
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
        self.b.switch_to_block(entry);
        for (local, decl) in body.locals.iter() {
            if borrowed[local.index()] || is_memory(sigs, decl.ty) {
                let address = self.new_slot(decl.ty);
                self.slots.push(Some(address));
                self.vars.push(SliceId::default());
            } else {
                let vars = self
                    .variable_lists
                    .alloc(leaves_of(sigs, decl.ty).into_iter().map(|leaf| {
                        self.b
                            .declare_var(keel_type(leaf).expect("leaves are scalars"))
                    }));
                self.slots.push(None);
                self.vars.push(vars);
            }
        }
        if let Return::Sret(layout) | Return::Memory(layout) =
            return_kind(sigs, &self.ctx.layouts, body.locals[Body::RETURN].ty)
        {
            let pointer = self.b.append_block_param(entry, Type::Ptr);
            self.sret = Some((pointer, layout));
        }
        for param in body.params() {
            let leaves = leaves_of(sigs, body.locals[param].ty);
            let values: Values = leaves
                .into_iter()
                .map(|leaf| {
                    let ty = keel_type(leaf).expect("leaves are scalars");
                    self.b.append_block_param(entry, ty)
                })
                .collect();
            self.write(&param.into(), &values);
        }
        if self.takes_location {
            self.caller_location = Some(self.b.append_block_param(entry, Type::Ptr));
        }
        self.b.ins().jump(self.blocks[0], &[]);
        for (block, data) in body.blocks.iter() {
            self.b.switch_to_block(self.blocks[block.index()]);
            for statement in &data.statements {
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    let ty = body.place_ty(dest, sigs);
                    if is_memory(sigs, ty)
                        && let Rvalue::Aggregate(..) | Rvalue::Repeat(..) = rvalue
                    {
                        let (location, _) = self.locate(dest);
                        let Location::Memory(address) = location else {
                            unreachable!("memory types are in memory")
                        };
                        self.build_in_memory(address, rvalue, ty);
                        continue;
                    }
                    let values = self.rvalue(rvalue, ty);
                    self.write(dest, &values);
                }
            }
            self.terminator(&data.terminator.kind, data.terminator.range.start(), block);
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
        let mut ty = PlaceTy::from(self.body.locals[place.local].ty);
        let mut location = match self.slots[place.local.index()] {
            Some(address) => Location::Memory(address),
            None => Location::Vars {
                local: place.local,
                start: 0,
                len: self.variable_lists[self.vars[place.local.index()]].len(),
            },
        };
        for (i, &elem) in place.projection.iter().enumerate() {
            let elem_ty = ty.project(elem, sigs);
            location = match (elem, location) {
                (ProjectionElem::Field(field), Location::Vars { local, start, .. }) => {
                    let fields = place_fields(sigs, ty);
                    let before: usize = fields[..field as usize]
                        .iter()
                        .map(|&f| leaf_count(sigs, f))
                        .sum();
                    Location::Vars {
                        local,
                        start: start + before,
                        len: leaf_count(sigs, elem_ty.ty),
                    }
                }
                (ProjectionElem::Field(field), Location::Memory(address)) => {
                    let layout = self.ctx.layouts.of(ty.ty);
                    let offset = layout.field_offset(ty.variant.unwrap_or(0), field);
                    Location::Memory(self.offset(address, offset))
                }
                (ProjectionElem::Downcast(variant), Location::Vars { local, start, .. }) => {
                    let variants = sigs.variants(ty.ty).expect("downcasts are of enums");
                    let count = |fields: &[Ty]| -> usize {
                        fields.iter().map(|&f| leaf_count(sigs, f)).sum()
                    };
                    let before: usize = variants[..variant as usize].iter().map(|f| count(f)).sum();
                    Location::Vars {
                        local,
                        start: start + 1 + before,
                        len: count(variants[variant as usize]),
                    }
                }
                // A variant's fields' offsets are from the enum's start.
                (ProjectionElem::Downcast(_), Location::Memory(address)) => {
                    Location::Memory(address)
                }
                (ProjectionElem::Deref | ProjectionElem::BoxDeref, location) => {
                    let pointer = self.load(location, ty.ty)[0];
                    Location::Memory(pointer)
                }
                (ProjectionElem::Index(index), Location::Memory(address)) => {
                    let size = self.ctx.layouts.of(elem_ty.ty).size;
                    let index = self.read(&index.into())[0];
                    Location::Memory(self.element(address, index, size))
                }
                (
                    ProjectionElem::ConstantIndex {
                        offset, from_end, ..
                    },
                    Location::Memory(address),
                ) => {
                    let size = self.ctx.layouts.of(elem_ty.ty).size;
                    let index = if from_end {
                        // `len - offset`, the length an array's or the
                        // slice's pointer's.
                        let len = match ty.ty.kind() {
                            TyKind::Array(_, n) => {
                                self.b.ins().iconst(Type::I64, array_len(n) as i64)
                            }
                            _ => {
                                let base = Place {
                                    local: place.local,
                                    projection: place.projection.prefix(i),
                                };
                                self.slice_parts(&base)[1]
                            }
                        };
                        let offset = self.b.ins().iconst(Type::I64, i64::from(offset));
                        self.b.ins().isub(len, offset)
                    } else {
                        self.b.ins().iconst(Type::I64, i64::from(offset))
                    };
                    Location::Memory(self.element(address, index, size))
                }
                (ProjectionElem::Subslice { from, .. }, Location::Memory(address)) => {
                    let elem = match ty.ty.kind() {
                        TyKind::Array(elem, _) | TyKind::Slice(elem) => elem,
                        _ => unreachable!("subslices are of arrays and slices"),
                    };
                    let size = self.ctx.layouts.of(elem).size;
                    Location::Memory(self.offset(address, u64::from(from) * size))
                }
                (
                    ProjectionElem::Index(_)
                    | ProjectionElem::ConstantIndex { .. }
                    | ProjectionElem::Subslice { .. },
                    Location::Vars { .. },
                ) => {
                    unreachable!("arrays are in memory")
                }
            };
            ty = elem_ty;
        }
        (location, ty.ty)
    }

    /// The pointer and length of the unsized place `place` (or the
    /// pointer and vtable of a trait object): those of the pointer it is
    /// behind, or for a slice's part (`rest @ ..`), moved past its first
    /// elements and shortened by those at both ends.
    fn slice_parts(&mut self, place: &Place) -> Values {
        match &*place.projection {
            [base @ .., ProjectionElem::Subslice { from, to, .. }] => {
                let base = Place {
                    local: place.local,
                    projection: place.projection.prefix(base.len()),
                };
                let parts = self.slice_parts(&base);
                let elem = match self.body.place_ty(&base, self.sigs).kind() {
                    TyKind::Slice(elem) => elem,
                    _ => unreachable!("a slice's part is of a slice"),
                };
                let size = self.ctx.layouts.of(elem).size;
                let pointer = self.offset(parts[0], u64::from(*from) * size);
                let cut = self.b.ins().iconst(Type::I64, i64::from(from + to));
                let len = self.b.ins().isub(parts[1], cut);
                smallvec![pointer, len]
            }
            _ => self.read(&pointer_of(place)),
        }
    }

    /// The address of element `index` of the array or slice at
    /// `address`, whose elements are `size` bytes apart.
    fn element(&mut self, address: Value, index: Value, size: u64) -> Value {
        if size == 0 {
            return address;
        }
        let size = self.b.ins().iconst(Type::I64, size as i64);
        let offset = self.b.ins().imul(index, size);
        self.b.ins().ptradd(address, offset)
    }

    /// Copies `size` bytes of a value of a memory type from `from` to `to`.
    fn copy(&mut self, to: Value, from: Value, size: u64) {
        if size == 0 {
            return;
        }
        let len = self.b.ins().iconst(Type::I64, size as i64);
        self.b.ins().intrinsic(Intrinsic::Memmove, &[to, from, len]);
    }

    /// The leaves of the `ty` at `location`: for a memory type, its
    /// address.
    fn load(&mut self, location: Location, ty: Ty) -> Values {
        if is_memory(self.sigs, ty) {
            let Location::Memory(address) = location else {
                unreachable!("memory types are in memory")
            };
            return smallvec![address];
        }
        match location {
            Location::Vars { local, start, len } => (start..start + len)
                .map(|i| {
                    let var = self.variable_lists[self.vars[local.index()]][i];
                    self.b.use_var(var)
                })
                .collect(),
            Location::Memory(address) => self.load_value(address, ty),
        }
    }

    fn read(&mut self, place: &Place) -> Values {
        let (location, ty) = self.locate(place);
        self.load(location, ty)
    }

    fn write(&mut self, place: &Place, values: &[Value]) {
        let (location, ty) = self.locate(place);
        match location {
            Location::Vars { local, start, len } => {
                for (i, &value) in (start..start + len).zip(values) {
                    let var = self.variable_lists[self.vars[local.index()]][i];
                    self.b.def_var(var, value);
                }
            }
            Location::Memory(address) if is_memory(self.sigs, ty) => {
                let size = self.ctx.layouts.of(ty).size;
                // A value with no size (`[T; 0]`) may come as no value.
                if let Some(&value) = values.first()
                    && size > 0
                {
                    self.copy(address, value, size);
                }
            }
            Location::Memory(address) => self.store(address, ty, values),
        }
    }

    /// A new stack slot for a value of `ty`, and its address.
    fn new_slot(&mut self, ty: Ty) -> Value {
        let layout = self.ctx.layouts.of(ty);
        self.aligned_slot(layout.size, layout.align)
    }

    /// Keel's stack frame is aligned to 16 bytes. Reserve padding and
    /// round an interior pointer for types with a larger alignment.
    fn aligned_slot(&mut self, size: u64, align: u64) -> Value {
        let padding = if align > 16 { align - 1 } else { 0 };
        let reserve = u32::try_from(
            size.max(1)
                .checked_add(padding)
                .expect("stack size overflow"),
        )
        .expect("stack object exceeds the IR size limit");
        let slot = self.b.create_stack_slot(reserve, align.min(16) as u32);
        let base = self.b.ins().stack_addr(slot);
        if align <= 16 {
            return base;
        }
        let bits = self.b.ins().convert(ConvertOp::PtrToInt, Type::I64, base);
        let padding = self.b.ins().iconst(Type::I64, padding as i64);
        let upper = self.b.ins().iadd(bits, padding);
        let mask = self.b.ins().iconst(Type::I64, -(align as i64));
        let rounded = self.b.ins().and(upper, mask);
        self.b
            .ins()
            .convert(ConvertOp::IntToPtr, Type::Ptr, rounded)
    }

    /// Stores at `address` the value of `rvalue`, an aggregate or repeat
    /// expression of `ty`, a memory type: each part where it goes.
    fn build_in_memory(&mut self, address: Value, rvalue: &Rvalue, ty: Ty) {
        let sigs = self.sigs;
        match rvalue {
            Rvalue::Aggregate(AggregateKind::Union(_, field), operands) => {
                let field_ty = sigs.fields(ty).unwrap()[*field as usize];
                let values = self.operand(&operands[0]);
                self.store(address, field_ty, &values);
            }
            Rvalue::Aggregate(AggregateKind::Array(elem), operands) => {
                let size = self.ctx.layouts.of(*elem).size;
                for (i, operand) in operands.iter().enumerate() {
                    let values = self.operand(operand);
                    let at = self.offset(address, size * i as u64);
                    self.store(at, *elem, &values);
                }
            }
            Rvalue::Aggregate(AggregateKind::Variant(strukt, index), operands) => {
                let layout = self.ctx.layouts.of(ty);
                let fields = sigs.variant_fields(ty, *index).unwrap_or_default();
                for (i, (operand, &field)) in operands.iter().zip(fields).enumerate() {
                    let values = self.operand(operand);
                    let at = self.offset(address, layout.field_offset(*index, i as u32));
                    self.store(at, field, &values);
                }
                self.store_tag(address, ty, *strukt, *index);
            }
            Rvalue::Aggregate(_, operands) => {
                let layout = self.ctx.layouts.of(ty);
                let fields = sigs.fields(ty).unwrap_or_default();
                for (i, (operand, &field)) in operands.iter().zip(fields).enumerate() {
                    let values = self.operand(operand);
                    let at = self.offset(address, layout.field_offset(0, i as u32));
                    self.store(at, field, &values);
                }
            }
            Rvalue::Repeat(operand, count) => {
                let TyKind::Array(elem, _) = ty.kind() else {
                    unreachable!("repeats make arrays")
                };
                let values = self.operand(operand);
                self.repeat(address, elem, &values, array_len(*count));
            }
            _ => unreachable!("only aggregates are built in memory"),
        }
    }

    /// Stores `values`, the leaves of a `ty`, at `address`, laid out as
    /// rustc lays it out; an enum's variant is the one its first leaf,
    /// the discriminant, says.
    fn store(&mut self, address: Value, ty: Ty, values: &[Value]) {
        let sigs = self.sigs;
        if is_memory(sigs, ty) {
            let size = self.ctx.layouts.of(ty).size;
            if let Some(&value) = values.first() {
                self.copy(address, value, size);
            }
            return;
        }
        if is_fat(ty) {
            self.b
                .ins()
                .store_with_flags(values[0], address, aligned(Type::Ptr));
            let at = self.offset(address, 8);
            self.b
                .ins()
                .store_with_flags(values[1], at, aligned(Type::I64));
            return;
        }
        if let Some(variants) = sigs.variants(ty) {
            self.store_enum(address, ty, &variants, values);
            return;
        }
        if let Some(fields) = sigs.fields(ty) {
            let layout = self.ctx.layouts.of(ty);
            let mut at = 0;
            for (i, &field) in fields.iter().enumerate() {
                let count = leaf_count(sigs, field);
                let to = self.offset(address, layout.field_offset(0, i as u32));
                self.store(to, field, &values[at..at + count]);
                at += count;
            }
            return;
        }
        if let (Some(scalar), Some(&value)) = (keel_type(ty), values.first()) {
            self.b
                .ins()
                .store_with_flags(value, address, aligned(scalar));
        }
    }

    /// Stores an enum's leaves: the fields of the variant its
    /// discriminant says, and its tag. Variants share their bytes, so
    /// only that variant's are stored: where several have fields, the
    /// discriminant picks which code runs.
    fn store_enum(&mut self, address: Value, ty: Ty, variants: &[&'static [Ty]], values: &[Value]) {
        let sigs = self.sigs;
        let layout = self.ctx.layouts.of(ty);
        let TyKind::Adt(adt, _) = ty.kind() else {
            return;
        };
        let strukt = adt.strukt;
        // Each variant's leaves, after the discriminant.
        let mut ranges = Vec::new();
        let mut at = 1;
        for fields in variants {
            let count: usize = fields.iter().map(|&f| leaf_count(sigs, f)).sum();
            ranges.push(at..at + count);
            at += count;
        }
        let store_variant = |this: &mut Self, v: usize| {
            let mut leaf = ranges[v].start;
            for (i, &field) in variants[v].iter().enumerate() {
                let count = leaf_count(sigs, field);
                let to = this.offset(address, layout.field_offset(v as u32, i as u32));
                this.store(to, field, &values[leaf..leaf + count]);
                leaf += count;
            }
        };
        let with_data: Vec<usize> = (0..variants.len())
            .filter(|&v| !ranges[v].is_empty())
            .collect();
        let tag = match &layout.shape {
            Shape::Enum { tag, present, .. } => tag.map(|t| (t, *present)),
            _ => None,
        };
        let Some((tag, _)) = tag else {
            // One variant may hold a value: it is that one.
            if let Shape::Enum { present, .. } = layout.shape {
                store_variant(self, present as usize);
            }
            return;
        };
        // One variant with fields, the tag outside them: its fields are
        // stored whatever the variant, which nothing else's overlap.
        if tag.encoding == TagEncoding::Direct && with_data.len() <= 1 {
            if let Some(&v) = with_data.first() {
                store_variant(self, v);
            }
            self.store_tag_of(address, ty, values[0]);
            return;
        }
        let join = self.b.create_block();
        let blocks: Vec<Block> = variants.iter().map(|_| self.b.create_block()).collect();
        let cases = (0..variants.len())
            .map(|v| {
                let bits = self.ctx.discriminant(strukt, v as u32);
                let call = BlockCall {
                    block: blocks[v],
                    args: Vec::new(),
                };
                (bits as i64, call)
            })
            .collect();
        let default = BlockCall {
            block: *blocks.last().expect("an enum with a tag has variants"),
            args: Vec::new(),
        };
        self.b.ins().switch(values[0], default, cases);
        for (v, &block) in blocks.iter().enumerate() {
            self.b.switch_to_block(block);
            store_variant(self, v);
            self.store_tag(address, ty, strukt, v as u32);
            self.b.ins().jump(join, &[]);
        }
        self.b.switch_to_block(join);
    }

    /// Stores the tag of variant `variant` of `ty`, an enum of `strukt`,
    /// at `address`, its fields there already.
    fn store_tag(&mut self, address: Value, ty: Ty, strukt: flux_hir::StructId, variant: u32) {
        if let Some((offset, bytes, bits)) = self.ctx.tag_store(ty, strukt, variant) {
            let int = tag_int(bytes);
            let value = self.int_const(bits, int);
            let at = self.offset(address, offset);
            self.b
                .ins()
                .store_with_flags(value, at, aligned(int_type(int)));
        }
    }

    /// Stores a direct tag, `discriminant` (of the enum's discriminant
    /// type) truncated to its width.
    fn store_tag_of(&mut self, address: Value, ty: Ty, value: Value) {
        let layout = self.ctx.layouts.of(ty);
        let Shape::Enum { tag: Some(tag), .. } = layout.shape else {
            return;
        };
        let from = discriminant(self.sigs, ty);
        let int = tag_int(tag.bytes);
        let value = if int.bits() < from.bits() {
            self.b.ins().trunc(int_type(int), value)
        } else {
            value
        };
        let at = self.offset(address, tag.offset);
        self.b
            .ins()
            .store_with_flags(value, at, aligned(int_type(int)));
    }

    /// The discriminant of the enum `ty` at `address`, of its
    /// discriminant type: the tag extended, or decoded from the niche.
    fn load_discriminant(&mut self, address: Value, ty: Ty) -> Value {
        let layout = self.ctx.layouts.of(ty);
        let leaf = discriminant(self.sigs, ty);
        let TyKind::Adt(adt, _) = ty.kind() else {
            return self.int_const(0, leaf);
        };
        let strukt = adt.strukt;
        let (tag, present) = match &layout.shape {
            Shape::Enum { tag, present, .. } => (*tag, *present),
            _ => (None, 0),
        };
        let Some(tag) = tag else {
            let bits = self.ctx.discriminant(strukt, present);
            return self.int_const(bits, leaf);
        };
        let int = tag_int(tag.bytes);
        let at = self.offset(address, tag.offset);
        let raw = self
            .b
            .ins()
            .load_with_flags(int_type(int), at, aligned(int_type(int)));
        // `raw` in the discriminant's type.
        let widen = |this: &mut Self, value: Value, signed: bool| -> Value {
            if leaf.bits() > int.bits() {
                if signed {
                    this.b.ins().sext(int_type(leaf), value)
                } else {
                    this.b.ins().uext(int_type(leaf), value)
                }
            } else if leaf.bits() < int.bits() {
                this.b.ins().trunc(int_type(leaf), value)
            } else {
                value
            }
        };
        match tag.encoding {
            TagEncoding::Direct => widen(self, raw, tag.signed),
            TagEncoding::Niche {
                untagged,
                first,
                last,
                start,
            } => {
                let start = self.int_const(start, int);
                let relative = self.b.ins().isub(raw, start);
                let span = self.int_const(u128::from(last - first), int);
                let in_niche = self.b.ins().icmp(IntCC::Ule, relative, span);
                let untagged_bits = self.ctx.discriminant(strukt, untagged);
                let mut result = self.int_const(untagged_bits, leaf);
                for v in first..=last {
                    if v == untagged {
                        continue;
                    }
                    let index = self.int_const(u128::from(v - first), int);
                    let is = self.b.ins().icmp(IntCC::Eq, relative, index);
                    let is = self.b.ins().and(is, in_niche);
                    let bits = self.ctx.discriminant(strukt, v);
                    let value = self.int_const(bits, leaf);
                    result = self.b.ins().select(is, value, result);
                }
                result
            }
        }
    }

    /// The leaves of a `ty` at `address`: for an enum, its discriminant,
    /// then every variant's fields, those of the variants it does not
    /// hold meaningless.
    fn load_value(&mut self, address: Value, ty: Ty) -> Values {
        let mut out = Values::new();
        self.load_into(address, ty, &mut out);
        out
    }

    fn load_into(&mut self, address: Value, ty: Ty, out: &mut Values) {
        let sigs = self.sigs;
        if is_memory(sigs, ty) {
            out.push(address);
            return;
        }
        if is_fat(ty) {
            out.push(
                self.b
                    .ins()
                    .load_with_flags(Type::Ptr, address, aligned(Type::Ptr)),
            );
            let at = self.offset(address, 8);
            let meta = if is_dyn_ptr(ty) { Type::Ptr } else { Type::I64 };
            out.push(self.b.ins().load_with_flags(meta, at, aligned(meta)));
            return;
        }
        if let Some(variants) = sigs.variants(ty) {
            out.push(self.load_discriminant(address, ty));
            let layout = self.ctx.layouts.of(ty);
            for (v, fields) in variants.iter().enumerate() {
                for (i, &field) in fields.iter().enumerate() {
                    let at = self.offset(address, layout.field_offset(v as u32, i as u32));
                    self.load_into(at, field, out);
                }
            }
            return;
        }
        if let Some(fields) = sigs.fields(ty) {
            let layout = self.ctx.layouts.of(ty);
            for (i, &field) in fields.iter().enumerate() {
                let at = self.offset(address, layout.field_offset(0, i as u32));
                self.load_into(at, field, out);
            }
            return;
        }
        if let Some(scalar) = keel_type(ty) {
            out.push(
                self.b
                    .ins()
                    .load_with_flags(scalar, address, aligned(scalar)),
            );
        }
    }

    /// Stores `count` copies of `values`, the leaves of an `elem`, from
    /// `address` on: unrolled when there are few, else in a loop.
    fn repeat(&mut self, address: Value, elem: Ty, values: &[Value], count: u64) {
        let size = self.ctx.layouts.of(elem).size;
        if size == 0 {
            return;
        }
        if count <= 8 {
            for i in 0..count {
                let at = self.offset(address, size * i);
                self.store(at, elem, values);
            }
            return;
        }
        // for (p = address; p != end; p += size) *p = value
        let head = self.b.create_block();
        let body = self.b.create_block();
        let exit = self.b.create_block();
        let total = self.b.ins().iconst(Type::I64, (size * count) as i64);
        let end = self.b.ins().ptradd(address, total);
        self.b.ins().jump(head, &[address]);
        self.b.switch_to_block(head);
        let at = self.b.append_block_param(head, Type::Ptr);
        let done = self.b.ins().icmp(IntCC::Eq, at, end);
        self.b.ins().brif(done, exit, &[], body, &[]);
        self.b.switch_to_block(body);
        self.store(at, elem, values);
        let next = self.offset(at, size);
        self.b.ins().jump(head, &[next]);
        self.b.switch_to_block(exit);
    }

    /// An operand's leaves.
    fn operand(&mut self, operand: &Operand) -> Values {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read(place),
            Operand::Const(Constant::Item { id, args, ty }) => {
                let c = flux_ty::ConstInstance { konst: *id, args };
                let known = self.ctx.const_values.get(*id, args);
                let value = match known.or_else(|| self.ctx.instance_values.get(&c)) {
                    Some(value) => value.clone(),
                    None => match self.ctx.evaluator.eval_instance(c) {
                        Ok(value) => {
                            self.ctx.instance_values.insert(c, value.clone());
                            value
                        }
                        // Reported once code generation is done.
                        Err(_) => return self.zeroes(*ty),
                    },
                };
                if is_memory(self.sigs, *ty) {
                    let address = self.new_slot(*ty);
                    self.store_value(address, &value, *ty);
                    return smallvec![address];
                }
                let mut leaves = Values::new();
                self.value_leaves(&value, *ty, &mut leaves);
                leaves
            }
            // A reference to a slice or `str` is two leaves.
            Operand::Const(Constant::Promoted { index, ty }) => {
                let value = flux_mir::eval::Value::Ref(Arc::new(
                    self.body.promoted[*index as usize].clone(),
                ));
                let mut leaves = Values::new();
                self.value_leaves(&value, *ty, &mut leaves);
                leaves
            }
            Operand::Const(constant) => self.constant(*constant).into_iter().collect(),
        }
    }

    /// Stores `value`, a constant of type `ty`, at `address`.
    fn store_value(&mut self, address: Value, value: &flux_mir::eval::Value, ty: Ty) {
        use flux_mir::eval::Value as V;
        let sigs = self.sigs;
        if !is_memory(sigs, ty) {
            let mut leaves = Values::new();
            self.value_leaves(value, ty, &mut leaves);
            self.store(address, ty, &leaves);
            return;
        }
        let layout = self.ctx.layouts.of(ty);
        match (value, ty.kind()) {
            (V::Aggregate(elements), TyKind::Array(elem, _)) => {
                let size = self.ctx.layouts.of(elem).size;
                for (i, element) in elements.iter().enumerate() {
                    let at = self.offset(address, size * i as u64);
                    self.store_value(at, element, elem);
                }
            }
            (V::Aggregate(fields), _) => {
                let types = sigs.fields(ty).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let at = self.offset(address, layout.field_offset(0, i as u32));
                    self.store_value(at, field, field_ty);
                }
            }
            (
                V::Variant {
                    strukt,
                    index,
                    fields,
                },
                _,
            ) => {
                let types = sigs.variant_fields(ty, *index).unwrap_or_default();
                for (i, (field, &field_ty)) in fields.iter().zip(types).enumerate() {
                    let at = self.offset(address, layout.field_offset(*index, i as u32));
                    self.store_value(at, field, field_ty);
                }
                self.store_tag(address, ty, *strukt, *index);
            }
            _ => unreachable!("memory types are aggregates"),
        }
    }

    /// Zero leaves of type `ty`: a value for code that is never output.
    fn zeroes(&mut self, ty: Ty) -> Values {
        if is_memory(self.sigs, ty) {
            return smallvec![self.new_slot(ty)];
        }
        leaves_of(self.sigs, ty)
            .into_iter()
            .filter_map(keel_type)
            .map(|t| {
                // keel has no pointer constants: a null pointer is an
                // integer converted.
                if t == Type::Ptr {
                    let zero = self.b.ins().iconst(Type::I64, 0);
                    self.b
                        .ins()
                        .convert(keel_ir::ConvertOp::IntToPtr, Type::Ptr, zero)
                } else {
                    self.b.ins().iconst(t, 0)
                }
            })
            .collect()
    }

    /// A constant item's value, of type `ty`, as leaves.
    fn value_leaves(&mut self, value: &flux_mir::eval::Value, ty: Ty, out: &mut Values) {
        use flux_mir::eval::Value as V;
        match value {
            V::Int { bits, ty } => out.push(self.int_const(*bits, *ty)),
            V::Bool(b) => out.push(self.b.ins().iconst(Type::I8, i64::from(*b))),
            V::Static(id) => out.push(self.static_addr(*id)),
            V::Ref(pointee) => {
                let (symbol, len) = self.ctx.referenced(pointee, ty);
                let data = self.b.import_data(&symbol);
                out.push(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)));
                if let Some(len) = len {
                    out.push(self.int_const(u128::from(len), IntTy::Usize));
                }
            }
            V::Aggregate(fields) => {
                let types = self.sigs.fields(ty).unwrap_or_default();
                for (field, &field_ty) in fields.iter().zip(types) {
                    self.value_leaves(field, field_ty, out);
                }
            }
            V::Variant {
                strukt,
                index,
                fields,
            } => {
                let bits = self.ctx.discriminant(*strukt, *index);
                let int = flux_ty::discriminant_ty(&self.ctx.hir.items, *strukt);
                out.push(self.int_const(bits, int));
                let variants = self.sigs.variants(ty).unwrap_or_default();
                for (v, types) in variants.into_iter().enumerate() {
                    if v as u32 == *index {
                        for (field, &field_ty) in fields.iter().zip(types) {
                            self.value_leaves(field, field_ty, out);
                        }
                    } else {
                        for &field_ty in types {
                            out.extend(self.zeroes(field_ty));
                        }
                    }
                }
            }
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
            Constant::Char(c) => Some(self.b.ins().iconst(Type::I32, i64::from(u32::from(c)))),
            Constant::Unit | Constant::FnItem(_) => None,
            Constant::CStr(index) => {
                let name = self.ctx.c_string(&self.body.c_strings[index as usize]);
                let data = self.b.import_data(&name);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)))
            }
            Constant::Item { .. } => unreachable!("handled by `operand`"),
            Constant::TyConst { .. } => unreachable!("monomorphized constants are known"),
            Constant::Static { id, .. } => Some(self.static_addr(id)),
            Constant::Promoted { index, ty } => {
                let pointee = match ty.kind() {
                    TyKind::Ref(_, pointee) => pointee,
                    _ => unreachable!("promoted constants are references"),
                };
                let value = self.body.promoted[index as usize].clone();
                let name = self.ctx.promoted(&value, pointee);
                let data = self.b.import_data(&name);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Data(data)))
            }
            Constant::Fn { function, args, .. } => {
                let callee = self.callee(function, args);
                Some(self.b.ins().symbol_addr(keel_ir::Symbol::Func(callee)))
            }
        }
    }

    /// The address of static `id`.
    fn static_addr(&mut self, id: flux_hir::ConstId) -> Value {
        let data = self.b.import_data(&self.ctx.statics[&id]);
        self.b.ins().symbol_addr(keel_ir::Symbol::Data(data))
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

    /// The leaves of an rvalue of type `ty`.
    fn rvalue(&mut self, rvalue: &Rvalue, ty: Ty) -> Values {
        match rvalue {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::Aggregate(AggregateKind::Variant(strukt, index), operands) => {
                let bits = self.ctx.discriminant(*strukt, *index);
                let int = flux_ty::discriminant_ty(&self.ctx.hir.items, *strukt);
                let mut leaves = smallvec![self.int_const(bits, int)];
                let variants = self.sigs.variants(ty).unwrap_or_default();
                for (v, types) in variants.into_iter().enumerate() {
                    if v as u32 == *index {
                        for operand in operands {
                            leaves.extend(self.operand(operand));
                        }
                    } else {
                        for &field_ty in types {
                            leaves.extend(self.zeroes(field_ty));
                        }
                    }
                }
                leaves
            }
            Rvalue::Aggregate(_, operands) => {
                operands.iter().flat_map(|o| self.operand(o)).collect()
            }
            Rvalue::Discriminant(place) => {
                let (location, _) = self.locate(place);
                let tag = match location {
                    Location::Vars { local, start, .. } => {
                        let var = self.variable_lists[self.vars[local.index()]][start];
                        self.b.use_var(var)
                    }
                    Location::Memory(address) => {
                        let enum_ty = self.body.place_ty(place, self.sigs);
                        self.load_discriminant(address, enum_ty)
                    }
                };
                smallvec![tag]
            }
            // A reference to a slice is the pointer it is behind.
            Rvalue::Ref { place, .. } if self.body.place_ty(place, self.sigs).is_unsized() => {
                self.slice_parts(place)
            }
            Rvalue::Ref { place, .. } => match self.locate(place).0 {
                Location::Memory(address) => smallvec![address],
                Location::Vars { .. } => unreachable!("borrowed locals are in memory"),
            },
            Rvalue::Len { place, .. } => match self.body.place_ty(place, self.sigs).kind() {
                TyKind::Array(_, len) => {
                    smallvec![self.int_const(u128::from(array_len(len)), IntTy::Usize)]
                }
                _ => smallvec![self.slice_parts(place)[1]],
            },
            // To a trait object: the pointer and the type's vtable; from
            // one, upcast, the pointer and the supertrait's vtable, which
            // the subtrait's has.
            Rvalue::Unsize(operand, to) if is_dyn_ptr(*to) || is_dyn_box(*to) => {
                let from = self.body.operand_ty(operand, self.sigs);
                let (Some(pointee_of), Some(object)) = (pointee_of(from), pointee_of(*to)) else {
                    unreachable!("unsizing is of pointers")
                };
                let leaves = self.operand(operand);
                if pointee_of.is_dyn() {
                    let index = flux_mir::vtable::super_index(
                        &self.ctx.hir.items,
                        self.sigs,
                        pointee_of,
                        object,
                    )
                    .expect("upcasting is to a supertrait");
                    let address = self.offset(leaves[1], 8 * index as u64);
                    let vtable =
                        self.b
                            .ins()
                            .load_with_flags(Type::Ptr, address, aligned(Type::Ptr));
                    return smallvec![leaves[0], vtable];
                }
                let name = self.ctx.vtable(object, pointee_of);
                let data = self.b.import_data(&name);
                let vtable = self.b.ins().symbol_addr(keel_ir::Symbol::Data(data));
                smallvec![leaves[0], vtable]
            }
            Rvalue::Unsize(operand, _) => {
                let from = self.body.operand_ty(operand, self.sigs);
                let len = match pointee_of(from).map(Ty::kind) {
                    Some(TyKind::Array(_, len)) => array_len(len),
                    _ => 0,
                };
                let pointer = self.operand(operand)[0];
                smallvec![pointer, self.int_const(u128::from(len), IntTy::Usize)]
            }
            // The address of element `start`, and `end - start` elements.
            Rvalue::Subslice { slice, start, end } => {
                let elem = match self.body.operand_ty(slice, self.sigs).kind() {
                    TyKind::Ref(_, s) => s.sequence_elem().unwrap_or(Ty::UNIT),
                    _ => Ty::UNIT,
                };
                let size = self.ctx.layouts.of(elem).size;
                let pointer = self.operand(slice)[0];
                let start = self.operand(start)[0];
                let end = self.operand(end)[0];
                let address = self.element(pointer, start, size);
                let len = self.b.ins().isub(end, start);
                smallvec![address, len]
            }
            Rvalue::Repeat(..) => unreachable!("arrays are built in memory"),
            // A pointer to a slice or `str` as one to a slice or `str`
            // keeps its length; as a thin pointer, drops it.
            Rvalue::Cast(operand, to) if is_fat(self.body.operand_ty(operand, self.sigs)) => {
                let mut leaves = self.operand(operand);
                if !is_fat(*to) {
                    leaves.truncate(1);
                }
                leaves
            }
            _ => self.scalar_rvalue(rvalue).into_iter().collect(),
        }
    }

    fn scalar_rvalue(&mut self, rvalue: &Rvalue) -> Option<Value> {
        match rvalue {
            Rvalue::Use(_)
            | Rvalue::Aggregate(..)
            | Rvalue::Ref { .. }
            | Rvalue::Discriminant(_)
            | Rvalue::Len { .. }
            | Rvalue::Repeat(..)
            | Rvalue::Unsize(..)
            | Rvalue::Subslice { .. } => {
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
                // Pointers are 64-bit addresses: a pointer as another is
                // itself, as an integer its address truncated, and an
                // integer as a pointer is extended to 64 bits first.
                let (from_ptr, to_ptr) = (from.is_pointer(), to.is_pointer());
                if from_ptr && to_ptr {
                    return Some(value);
                }
                if from_ptr {
                    let address =
                        self.b
                            .ins()
                            .convert(keel_ir::ConvertOp::PtrToInt, Type::I64, value);
                    return Some(match bits(*to) {
                        64 => address,
                        128 => self.b.ins().uext(to_ty, address),
                        _ => self.b.ins().trunc(to_ty, address),
                    });
                }
                if to_ptr {
                    let address = match bits(from) {
                        64 => value,
                        128 => self.b.ins().trunc(Type::I64, value),
                        _ if from.is_signed_int() => self.b.ins().sext(Type::I64, value),
                        _ => self.b.ins().uext(Type::I64, value),
                    };
                    return Some(self.b.ins().convert(
                        keel_ir::ConvertOp::IntToPtr,
                        Type::Ptr,
                        address,
                    ));
                }
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

    fn terminator(&mut self, kind: &TerminatorKind, offset: u32, block: flux_mir::BasicBlock) {
        match kind {
            TerminatorKind::Goto(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
                variants,
            } => {
                let targets: Vec<(u128, flux_mir::BasicBlock)> = match *variants {
                    Some(strukt) => targets
                        .iter()
                        .map(|&(v, block)| (self.ctx.discriminant(strukt, v as u32), block))
                        .collect(),
                    None => targets.clone(),
                };
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
                generic_args,
                args,
                dest,
                target,
            } if flux_mir::vtable::virtual_call(
                &self.ctx.hir.items,
                flux_ty::traits::resolve_instance(
                    &self.ctx.hir.items,
                    &self.ctx.typed.sigs,
                    Instance {
                        function: *func,
                        args: generic_args,
                    },
                ),
            )
            .is_some() =>
            {
                self.virtual_call(*func, generic_args, args, dest, *target);
            }
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } => {
                let items = &self.ctx.hir.items;
                let offset = self.body.fn_spans.get(&block).copied().unwrap_or(offset);
                if items.functions[*func].intrinsic
                    && items.functions[*func].name.as_str() == "caller_location"
                {
                    let location = self.location_here(offset);
                    self.write(dest, &[location]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    }
                    return;
                }
                if items.functions[*func].intrinsic
                    && flux_mir::mono::is_neon_intrinsic(items.functions[*func].name.as_str())
                {
                    self.neon_intrinsic(*func, generic_args, args, dest, *target);
                    return;
                }
                if items.functions[*func].intrinsic
                    && items.functions[*func].name.as_str() == "read_unaligned"
                {
                    // Validated after substitution by mono. No reference to the
                    // source is formed. Keel's default load flags promise only
                    // alignment one (and do not promise fault freedom).
                    let ty = keel_type(generic_args[0]).expect("validated unsigned scalar");
                    let address = self.operand(&args[0])[0];
                    let value = self.b.ins().load(ty, address);
                    self.write(dest, &[value]);
                    if let Some(target) = target {
                        self.b.ins().jump(self.blocks[target.index()], &[]);
                    } else {
                        self.b.ins().trap();
                    }
                    return;
                }
                // Of a trait object: through its vtable.
                if items.functions[*func].intrinsic {
                    self.dyn_intrinsic(*func, args, dest, *target);
                    return;
                }
                let callee = self.callee(*func, generic_args);
                let resolved = flux_ty::traits::resolve_instance(
                    items,
                    &self.ctx.typed.sigs,
                    Instance {
                        function: *func,
                        args: generic_args,
                    },
                );
                let location = self
                    .ctx
                    .takes_location(resolved)
                    .then(|| self.location_here(offset));
                let ret = flux_ty::traits::instance_ty(
                    &self.ctx.hir.items,
                    &self.ctx.typed.sigs,
                    self.ctx.typed.sigs[*func].ret,
                    generic_args,
                );
                self.call(ret, args, dest, *target, |b, values| match location {
                    Some(location) => {
                        let mut values = values.to_vec();
                        values.push(location);
                        b.ins().call(callee, &values)
                    }
                    None => b.ins().call(callee, values),
                });
            }
            TerminatorKind::CallIndirect {
                callee,
                args,
                dest,
                target,
            } => {
                let ty = self.body.operand_ty(callee, self.sigs);
                let sig = ty
                    .fn_sig()
                    .expect("indirect calls are of function pointers");
                let sig = FnSig {
                    params: sig.params().into(),
                    ret: sig.ret(),
                };
                let sig_ref =
                    self.b
                        .import_signature(signature(self.sigs, &self.ctx.layouts, &sig, false));
                let pointer = self.scalar(callee).expect("function pointers are scalars");
                self.call(sig.ret, args, dest, *target, |b, values| {
                    b.ins().call_indirect(sig_ref, pointer, values)
                });
            }
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target,
            } => {
                let target = self.blocks[target.index()];
                let Some(cond) = self.scalar(cond) else {
                    self.b.ins().jump(target, &[]);
                    return;
                };
                // `core`'s function for the check, as rustc's.
                let (lang, operands): (&str, Vec<&Operand>) = match msg {
                    AssertMessage::BoundsCheck { len, index } => {
                        ("panic_bounds_check", vec![index, len])
                    }
                    AssertMessage::DivisionByZero(_) => ("panic_const_div_by_zero", vec![]),
                    AssertMessage::RemainderByZero(_) => ("panic_const_rem_by_zero", vec![]),
                    AssertMessage::DivisionOverflow(..) => ("panic_const_div_overflow", vec![]),
                    AssertMessage::RemainderOverflow(..) => ("panic_const_rem_overflow", vec![]),
                };
                let lang_fn = self
                    .ctx
                    .hir
                    .items
                    .lang_fns
                    .get(&flux_intern::Symbol::intern(lang))
                    .copied();
                let panic = match lang_fn {
                    Some(_) => self.b.create_block(),
                    None => self.panic_block(),
                };
                if *expected {
                    self.b.ins().brif(cond, target, &[], panic, &[]);
                } else {
                    self.b.ins().brif(cond, panic, &[], target, &[]);
                }
                if let Some(function) = lang_fn {
                    self.b.switch_to_block(panic);
                    let mut values = Vec::new();
                    for operand in operands {
                        values.extend(self.operand(operand));
                    }
                    values.push(self.location_here(offset));
                    let callee = self.callee(function, Ty::list(&[]));
                    self.b.ins().call(callee, &values);
                    self.b.ins().trap();
                }
            }
            TerminatorKind::Return => {
                let values = self.read(&Body::RETURN.into());
                let ret = self.body.locals[Body::RETURN].ty;
                match self.sret.clone() {
                    Some((pointer, _)) if is_memory(self.sigs, ret) => {
                        let size = self.ctx.layouts.of(ret).size;
                        self.copy(pointer, values[0], size);
                        self.b.ins().return_(&[]);
                    }
                    Some((pointer, layout)) => {
                        for ((offset, ty), value) in layout.leaves().into_iter().zip(values) {
                            let address = self.offset(pointer, offset);
                            self.b.ins().store_with_flags(value, address, aligned(ty));
                        }
                        self.b.ins().return_(&[]);
                    }
                    // A value with no size returns nothing.
                    None if is_memory(self.sigs, ret) => {
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
            TerminatorKind::Drop { .. } => unreachable!("drops are expanded into calls"),
        }
    }

    /// A call returning a `ret`, with arguments `args`, its result
    /// stored in `dest`, continuing at `target`: `make` makes the call
    /// instruction from the argument values, an `sret` pointer first.
    fn call(
        &mut self,
        ret: Ty,
        args: &[flux_mir::Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: impl FnOnce(&mut FunctionBuilder<'_>, &[Value]) -> keel_ir::Inst,
    ) {
        let sret = self.sret_for(ret);
        let mut values: Values = sret.iter().map(|(pointer, _)| *pointer).collect();
        for arg in args {
            values.extend(self.operand(arg));
        }
        self.finish_call(ret, sret, values, dest, target, make);
    }

    /// Memory for a call's result of type `ret`, if it is returned
    /// through memory.
    fn sret_for(&mut self, ret: Ty) -> Option<(Value, Layout)> {
        match return_kind(self.sigs, &self.ctx.layouts, ret) {
            Return::Sret(layout) | Return::Memory(layout) => {
                let align = layout.align().max(self.ctx.layouts.of(ret).align);
                let address = self.aligned_slot(layout.size(), align);
                Some((address, layout))
            }
            Return::Values(_) => None,
        }
    }

    /// [`Self::call`] with the arguments' leaves given: `first`, then
    /// `rest`.
    fn call_values(
        &mut self,
        ret: Ty,
        first: Value,
        rest: Values,
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: impl FnOnce(&mut FunctionBuilder<'_>, &[Value]) -> keel_ir::Inst,
    ) {
        let sret = self.sret_for(ret);
        let mut values: Values = sret.iter().map(|(pointer, _)| *pointer).collect();
        values.push(first);
        values.extend(rest);
        self.finish_call(ret, sret, values, dest, target, make);
    }

    /// The call instruction `make` makes from `values`, its result
    /// stored in `dest` (through `sret` if given), continuing at
    /// `target`.
    fn finish_call(
        &mut self,
        ret: Ty,
        sret: Option<(Value, Layout)>,
        values: Values,
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
        make: impl FnOnce(&mut FunctionBuilder<'_>, &[Value]) -> keel_ir::Inst,
    ) {
        let memory = is_memory(self.sigs, ret);
        let call = make(&mut self.b, &values);
        // A call that does not return stores nothing: its destination may
        // be of any type (a `match` arm calling `panic`).
        if target.is_some() {
            let results = match &sret {
                Some((pointer, _)) if memory => smallvec![*pointer],
                Some((pointer, layout)) => self.load_leaves(*pointer, layout),
                None => self
                    .b
                    .function()
                    .inst_results(call)
                    .iter()
                    .copied()
                    .collect(),
            };
            self.write(dest, &results);
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

    /// The leaves of a value laid out as `layout` at `pointer`.
    fn load_leaves(&mut self, pointer: Value, layout: &Layout) -> Values {
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

    /// Where code at `offset` in the body's file is, for a
    /// `#[track_caller]` callee: where the function was called from if
    /// it takes that, as rustc passes it on.
    fn location_here(&mut self, offset: u32) -> Value {
        if let Some(location) = self.caller_location {
            return location;
        }
        let name = self.ctx.location(self.file, offset);
        let data = self.b.import_data(&name);
        self.b.ins().symbol_addr(keel_ir::Symbol::Data(data))
    }

    fn callee(&mut self, func: FunctionId, generic_args: GenericArgs) -> FuncRef {
        let instance = Instance {
            function: func,
            args: generic_args,
        };
        let instance =
            flux_ty::traits::resolve_instance(&self.ctx.hir.items, &self.ctx.typed.sigs, instance);
        let (func, generic_args) = (instance.function, instance.args);
        if let Some(&callee) = self.callees.get(&instance) {
            return callee;
        }
        let function = &self.ctx.hir.items.functions[func];
        let sig = flux_ty::traits::instance_sig(
            &self.ctx.hir.items,
            &self.ctx.typed.sigs,
            func,
            generic_args,
        );
        let mut sig = if function.is_extern {
            signature(self.sigs, &self.ctx.layouts, &sig, function.is_variadic)
        } else {
            let reified = self.ctx.reified.contains(&instance);
            ctx_signature(self.sigs, &self.ctx.layouts, &sig, reified)
        };
        if self.ctx.takes_location(instance) {
            sig.params.push(AbiParam::new(Type::Ptr));
        }
        let symbol = instance_symbol(self.ctx.hir, self.ctx.symbols, instance);
        let callee = self.b.import_function(&symbol, sig);
        self.callees.insert(instance, callee);
        callee
    }

    /// A call of trait method `func` on a trait object: the method is
    /// the vtable's, called through a pointer with C's convention, and
    /// the receiver, a pointer to the object, is passed as its address
    /// alone, as the method's `&self` for the type behind it is.
    fn virtual_call(
        &mut self,
        func: FunctionId,
        generic_args: GenericArgs,
        args: &[flux_mir::Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        let items = &self.ctx.hir.items;
        let sigs = &self.ctx.typed.sigs;
        let object = generic_args[0];
        let index = flux_mir::vtable::method_index(items, sigs, object, func)
            .expect("a trait object's method has a slot");
        let sig = flux_ty::traits::instance_sig(items, sigs, func, generic_args);
        // The receiver as a thin pointer.
        let mut params = sig.params.to_vec();
        params[0] = address_leaf();
        let thin = FnSig {
            params: params.into(),
            ret: sig.ret,
        };
        let sig_ref =
            self.b
                .import_signature(signature(self.sigs, &self.ctx.layouts, &thin, false));
        let receiver = self.operand(&args[0]);
        let address = self.offset(receiver[1], 8 * index as u64);
        let pointer = self
            .b
            .ins()
            .load_with_flags(Type::Ptr, address, aligned(Type::Ptr));
        let data = receiver[0];
        let rest = &args[1..];
        let mut leaves = Values::new();
        for arg in rest {
            leaves.extend(self.operand(arg));
        }
        self.call_values(sig.ret, data, leaves, dest, target, |b, values| {
            b.ins().call_indirect(sig_ref, pointer, values)
        });
    }

    /// NEON values use the ordinary indirect array aggregate convention at
    /// Rust calls; only the operation itself introduces packed Keel values.
    fn neon_intrinsic(
        &mut self,
        func: FunctionId,
        generic_args: &[Ty],
        args: &[Operand],
        dest: &Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        use keel_ir::{NeonIntrinsic, VectorBinaryOp, VectorType};
        let symbol = self.ctx.hir.items.functions[func].name;
        let name = symbol
            .as_str()
            .strip_prefix("__neon_")
            .expect("NEON intrinsic");
        let first = self.operand(&args[0])[0];
        let bytes = Type::Vector(VectorType::I8x16);
        let value = match name {
            "vshrn_n_u16" => {
                let TyKind::Value(shift) = generic_args[0].kind() else {
                    unreachable!("validated constant shift")
                };
                let input = self.b.ins().load(Type::Vector(VectorType::I16x8), first);
                let shift = self.b.ins().iconst(Type::I32, shift.bits() as i64);
                // A D-vector result keeps the store at exactly eight bytes.
                self.b.ins().neon(
                    NeonIntrinsic::Shrn,
                    Type::Vector(VectorType::I8x8),
                    &[input, shift],
                )
            }
            "vpmaxq_u8" => {
                let second = self.operand(&args[1])[0];
                let a = self.b.ins().load(bytes, first);
                let b = self.b.ins().load(bytes, second);
                self.b.ins().vector_binary(VectorBinaryOp::Pumax, a, b)
            }
            "vdupq_n_u8" => self.b.ins().vsplat(bytes, first),
            "vget_lane_u64" => self.b.ins().load(Type::I64, first),
            "vgetq_lane_u64" => {
                let TyKind::Value(lane) = generic_args[0].kind() else {
                    unreachable!("validated constant lane")
                };
                let vector = self.b.ins().load(Type::Vector(VectorType::I64x2), first);
                self.b.ins().vextract(vector, lane.bits() as u8)
            }
            _ => {
                // The load intrinsic explicitly promises no alignment. Using
                // the same conservative flags for aggregate inputs is valid.
                let a = self.b.ins().load(bytes, first);
                if name == "vld1q_u8" {
                    a
                } else {
                    let second = self.operand(&args[1])[0];
                    let b = self.b.ins().load(bytes, second);
                    match name {
                        "vceqq_u8" => self.b.ins().vicmp(IntCC::Eq, a, b),
                        "vandq_u8" => self.b.ins().vector_binary(VectorBinaryOp::And, a, b),
                        "vorrq_u8" => self.b.ins().vector_binary(VectorBinaryOp::Or, a, b),
                        _ => unreachable!("bounded NEON intrinsic"),
                    }
                }
            }
        };
        if matches!(name, "vgetq_lane_u64" | "vget_lane_u64") {
            self.write(dest, &[value]);
        } else {
            let (Location::Memory(address), _) = self.locate(dest) else {
                unreachable!("NEON aggregates live in memory")
            };
            self.b.ins().store(value, address);
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

    /// `size_of_val`, `align_of_val` or `drop_in_place` of a trait
    /// object, which `mono` leaves to code generation: the vtable's
    /// size or alignment word, or a call of its drop function, if it has
    /// one, with the data pointer.
    fn dyn_intrinsic(
        &mut self,
        func: FunctionId,
        args: &[Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        let name = self.ctx.hir.items.functions[func].name;
        let leaves = self.operand(&args[0]);
        let (data, vtable) = (leaves[0], leaves[1]);
        let word = |this: &mut Self, index: u64, ty: Type| {
            let at = this.offset(vtable, 8 * index);
            this.b.ins().load_with_flags(ty, at, aligned(ty))
        };
        match name.as_str() {
            "size_of_val" => {
                let size = word(self, 1, Type::I64);
                self.write(dest, &[size]);
            }
            "align_of_val" => {
                let align = word(self, 2, Type::I64);
                self.write(dest, &[align]);
            }
            _ => {
                // A null slot: nothing to drop.
                let drop = word(self, 0, Type::Ptr);
                let address = self
                    .b
                    .ins()
                    .convert(keel_ir::ConvertOp::PtrToInt, Type::I64, drop);
                let zero = self.b.ins().iconst(Type::I64, 0);
                let null = self.b.ins().icmp(IntCC::Eq, address, zero);
                let call = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(null, done, &[], call, &[]);
                self.b.switch_to_block(call);
                let sig = Signature::new(&[Type::Ptr], &[]);
                let sig_ref = self.b.import_signature(sig);
                self.b.ins().call_indirect(sig_ref, drop, &[data]);
                self.b.ins().jump(done, &[]);
                self.b.switch_to_block(done);
            }
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

/// The pointer `place`, a slice behind it, is behind.
fn pointer_of(place: &Place) -> Place {
    match &*place.projection {
        [rest @ .., ProjectionElem::Deref | ProjectionElem::BoxDeref] => Place {
            local: place.local,
            projection: place.projection.prefix(rest.len()),
        },
        _ => unreachable!("slices are behind pointers"),
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
