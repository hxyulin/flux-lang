//! Rust and native C call signature classification and admission.
use super::{
    AbiParam, ArgExt, CallConv, Diagnostic, FileMir, FnSig, FunctionAttrs, Hir, Instance, Layout,
    Layouts, Purpose, Signature, Signatures, TerminatorKind, Ty, TyKind, Type, TypedFile,
    bytes_layout, is_fat, is_memory, keel_type, leaf_count, leaves_of,
};

/// How a value of `ty` crosses a call: narrow integers are extended by
/// their signedness, as Apple's arm64 ABI requires of C callers.
pub(super) fn abi_param(ty: Ty) -> Option<AbiParam> {
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

pub(super) fn signature(
    sigs: &Signatures,
    layouts: &Layouts,
    sig: &FnSig,
    variadic: bool,
) -> Signature {
    Convention::RustReified.signature(sigs, layouts, sig, variadic)
}

/// A source convention, separate from Keel's final machine call convention.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Convention {
    RustFast,
    RustReified,
    C,
}

pub(super) enum Argument {
    Ignore,
    Scalar(Ty),
    Direct,
    Indirect,
    ByVal(Layout),
}

impl Convention {
    pub fn is_c(self) -> bool {
        self == Self::C
    }

    /// The same classification drives declaration, caller and callee lowering.
    pub fn argument(self, sigs: &Signatures, layouts: &Layouts, ty: Ty) -> Argument {
        if self.is_c()
            && let Some(scalar) = c_scalar(sigs, layouts, ty)
        {
            Argument::Scalar(scalar)
        } else if self.is_c() && c_aggregate(ty) {
            let layout =
                c_layout(sigs, layouts, ty).expect("C ABI preflight checked the parameter layout");
            if layout.size() == 0 {
                Argument::Ignore
            } else {
                Argument::ByVal(layout)
            }
        } else if !self.is_c() && indirect_argument(sigs, ty) {
            Argument::Indirect
        } else {
            Argument::Direct
        }
    }

    pub fn result(self, sigs: &Signatures, layouts: &Layouts, ty: Ty) -> Return {
        if self.is_c() {
            c_return_kind(sigs, layouts, ty)
        } else {
            return_kind(sigs, layouts, ty, self == Self::RustFast)
        }
    }

    pub fn signature(
        self,
        sigs: &Signatures,
        layouts: &Layouts,
        sig: &FnSig,
        variadic: bool,
    ) -> Signature {
        let mut signature = Signature::new(&[], &[]);
        match self.result(sigs, layouts, sig.ret) {
            Return::Values(returns) => signature.returns = returns,
            Return::Sret(layout) | Return::Memory(layout) => signature.params.push(AbiParam {
                purpose: Purpose::Sret(layout),
                ..AbiParam::new(Type::Ptr)
            }),
        }
        for &ty in &sig.params {
            match self.argument(sigs, layouts, ty) {
                Argument::Ignore => {}
                Argument::Scalar(ty) => signature.params.push(abi_param(ty).unwrap()),
                Argument::Direct => signature
                    .params
                    .extend(leaves_of(sigs, ty).into_iter().filter_map(abi_param)),
                Argument::Indirect => signature.params.push(AbiParam::new(Type::Ptr)),
                Argument::ByVal(layout) => signature.params.push(AbiParam {
                    purpose: Purpose::ByVal(layout),
                    ..AbiParam::new(Type::Ptr)
                }),
            }
        }
        signature.variadic = variadic;
        if self == Self::RustFast {
            signature.cc = CallConv::Fast;
            for param in signature.params.iter_mut().chain(&mut signature.returns) {
                param.ext = ArgExt::None;
            }
        }
        signature
    }
}

/// C aggregate contracts describe physical fields, not Rust's flattened leaves.
/// Keel has natural C layout only; reject shapes it cannot express exactly.
fn c_layout(sigs: &Signatures, layouts: &Layouts, ty: Ty) -> Option<Layout> {
    let actual = layouts.of(ty);
    if !actual.exact || is_fat(sigs, ty) {
        return None;
    }
    let layout = if let Some(scalar) = c_scalar(sigs, layouts, ty) {
        Layout::Scalar(keel_type(scalar)?)
    } else {
        match ty.kind() {
            TyKind::Tuple([]) | TyKind::Never => Layout::Struct(Vec::new()),
            TyKind::Array(elem, count) => Layout::Array(
                Box::new(c_layout(sigs, layouts, elem)?),
                u32::try_from(count.as_usize()?).ok()?,
            ),
            TyKind::Adt(adt, _) => {
                if sigs.unions.contains(&adt.strukt) || flux_solve::variants(sigs, ty).is_some() {
                    return None;
                }
                let fields = flux_solve::fields(sigs, ty)?;
                let fields: Option<Vec<_>> = fields
                    .iter()
                    .map(|&field| c_layout(sigs, layouts, field))
                    .collect();
                let fields = fields?;
                let mut offset = 0u64;
                for (i, field) in fields.iter().enumerate() {
                    offset = offset.next_multiple_of(field.align());
                    if offset != actual.field_offset(0, i as u32) {
                        return None;
                    }
                    offset += field.checked_size()?;
                }
                Layout::Struct(fields)
            }
            _ => Layout::Scalar(keel_type(ty)?),
        }
    };
    (layout.checked_size()? == actual.size && layout.align() == actual.align).then_some(layout)
}

/// Physical scalar representation of admitted enum shapes. Source enum leaves
/// still contain decoded discriminants and payloads; call boundaries marshal
/// through the actual layout rather than exposing those internal leaves.
pub(super) fn c_scalar(sigs: &Signatures, layouts: &Layouts, ty: Ty) -> Option<Ty> {
    let variants = flux_solve::variants(sigs, ty)?;
    let actual = layouts.of(ty);
    if !actual.exact {
        return None;
    }
    let super::Shape::Enum { tag: Some(tag), .. } = actual.shape else {
        return None;
    };
    let scalar = if !variants.is_empty()
        && variants.iter().all(|v| v.is_empty())
        && matches!(tag.encoding, super::TagEncoding::Direct)
    {
        let int = super::tag_int(tag.bytes);
        Ty::int(if tag.signed {
            match tag.bytes {
                1 => flux_ty::IntTy::I8,
                2 => flux_ty::IntTy::I16,
                4 => flux_ty::IntTy::I32,
                8 => flux_ty::IntTy::I64,
                _ => return None,
            }
        } else {
            int
        })
    } else if variants.len() == 2
        && tag.offset == 0
        && tag.bytes == 8
        && matches!(tag.encoding, super::TagEncoding::Niche { first, last, start: 0, .. } if first == last)
        && variants.iter().filter(|v| v.is_empty()).count() == 1
    {
        let payload = variants.iter().find(|v| !v.is_empty())?;
        if payload.len() != 1 || !matches!(payload[0].kind(), TyKind::FnPtr(_)) {
            return None;
        }
        super::address_leaf()
    } else {
        return None;
    };
    let layout = Layout::Scalar(keel_type(scalar)?);
    (layout.size() == actual.size && layout.align() == actual.align).then_some(scalar)
}

fn c_aggregate(ty: Ty) -> bool {
    matches!(
        ty.kind(),
        TyKind::Adt(..) | TyKind::Tuple(_) | TyKind::Array(..)
    )
}

fn c_return_kind(sigs: &Signatures, layouts: &Layouts, ret: Ty) -> Return {
    if let Some(scalar) = c_scalar(sigs, layouts, ret) {
        Return::Values(vec![abi_param(scalar).unwrap()])
    } else if c_aggregate(ret) {
        let layout =
            c_layout(sigs, layouts, ret).expect("C ABI preflight checked the return layout");
        if layout.size() == 0 {
            Return::Values(Vec::new())
        } else {
            Return::Memory(layout)
        }
    } else {
        return_kind(sigs, layouts, ret, false)
    }
}

pub(super) fn c_signature(
    sigs: &Signatures,
    layouts: &Layouts,
    sig: &FnSig,
    variadic: bool,
) -> Signature {
    Convention::C.signature(sigs, layouts, sig, variadic)
}

fn c_admitted(items: &flux_hir::ItemTree, sigs: &Signatures, layouts: &Layouts, ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            let data = &items.structs[adt.strukt];
            // The core SIMD wrappers represent vectors, whose C register
            // classification differs from an integer struct of the same size.
            let vector = items.is_core(data.loc.module)
                && matches!(
                    items.modules[data.loc.module].name.as_str(),
                    "aarch64" | "x86_64"
                );
            if let Some(variants) = flux_solve::variants(sigs, ty) {
                return c_scalar(sigs, layouts, ty).is_some()
                    && ((variants.iter().any(|v| !v.is_empty())
                        && items.is_core(data.loc.module)
                        && data.name.as_str() == "Option"
                        && items.modules[data.loc.module].name.as_str() == "option")
                        || (variants.iter().all(|v| v.is_empty())
                            && (data.repr_c || data.repr.is_some())));
            }
            data.repr_c
                && !vector
                && !sigs.unions.contains(&adt.strukt)
                && flux_solve::variants(sigs, ty).is_none()
                && flux_solve::fields(sigs, ty).is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|&ty| c_admitted(items, sigs, layouts, ty))
                })
        }
        TyKind::Array(elem, _) => c_admitted(items, sigs, layouts, elem),
        TyKind::Tuple(fields) => fields.is_empty(),
        _ => true,
    }
}

fn c_contains_ref(sigs: &Signatures, ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Ref(..) => true,
        // Only stored values cross the ABI. Raw-pointer pointees are neither
        // passed nor inspected, and may form recursive type graphs.
        TyKind::RawPtr(..) => false,
        TyKind::Array(ty, _) | TyKind::Slice(ty) => c_contains_ref(sigs, ty),
        TyKind::Adt(..) => flux_solve::contents(sigs, ty)
            .is_some_and(|fields| fields.iter().any(|&ty| c_contains_ref(sigs, ty))),
        TyKind::Tuple(fields) => fields.iter().any(|&ty| c_contains_ref(sigs, ty)),
        _ => false,
    }
}

pub(super) fn check_c_abi(
    hir: &Hir,
    typed: &TypedFile,
    mir: &FileMir,
    instances: &[Instance],
) -> Vec<Diagnostic> {
    let layouts = Layouts::new(&hir.items, &typed.sigs, &mir.discriminants);
    let supported = |sig: &FnSig| {
        !sig.params.iter().any(|&ty| c_contains_ref(&typed.sigs, ty))
            && sig.params.iter().copied().chain([sig.ret]).all(|ty| {
                c_admitted(&hir.items, &typed.sigs, &layouts, ty)
                    && c_layout(&typed.sigs, &layouts, ty).is_some()
            })
    };
    let mut errors = Vec::new();
    let mut add = |file, range| {
        let mut error =
            Diagnostic::new(range, "this type layout in the C ABI is not supported yet");
        error.file = file;
        errors.push(error);
    };
    for (id, data) in hir
        .items
        .functions
        .iter()
        .filter(|(_, data)| data.is_extern && data.abi.is_c())
    {
        if !supported(&typed.sigs[id]) {
            add(data.loc.file, data.def_range);
        }
    }
    for &instance in instances {
        let data = &hir.items.functions[instance.function];
        if data.abi.is_c()
            && !supported(&flux_solve::traits::instance_sig(
                &hir.items,
                &typed.sigs,
                instance.function,
                instance.args,
            ))
        {
            add(data.loc.file, data.def_range);
        }
        let body = flux_mir::mono::body_of(hir, &typed.sigs, mir, instance);
        let file = hir.items.functions[body.owner.unwrap_or(instance.function)]
            .loc
            .file;
        for block in body.blocks.values() {
            match &block.terminator.kind {
                TerminatorKind::CallIndirect { callee, .. } => {
                    if let Some(sig) = body.operand_ty(callee, &typed.sigs).fn_sig()
                        && sig.abi.is_c()
                        && !supported(&FnSig {
                            params: sig.params().into(),
                            ret: sig.ret(),
                        })
                    {
                        add(file, block.terminator.range);
                    }
                }
                TerminatorKind::Call { func, args, .. }
                    if hir.items.functions[*func].abi.is_c()
                        && hir.items.functions[*func].is_variadic =>
                {
                    let fixed = typed.sigs[*func].params.len();
                    if args
                        .iter()
                        .skip(fixed)
                        .any(|arg| c_aggregate(body.operand_ty(arg, &typed.sigs)))
                    {
                        add(file, block.terminator.range);
                    }
                }
                _ => {}
            }
        }
    }
    Diagnostic::sort(&mut errors);
    errors.dedup();
    errors
}

pub(super) fn attrs(
    sig: &FnSig,
    strategy: flux_mir::PanicStrategy,
    abi: flux_hir::Abi,
) -> FunctionAttrs {
    FunctionAttrs {
        noreturn: sig.ret == Ty::NEVER,
        nounwind: strategy == flux_mir::PanicStrategy::Abort || !abi.can_unwind(),
        ..FunctionAttrs::default()
    }
}

/// The signature of a function flux defines: keel's private convention
/// unless its address is taken (`reified`), when it must be called
/// through a pointer, which `cc(fast)` does not allow. Reification uses the
/// C machine convention while keeping Rust's aggregate representation.
pub(super) fn ctx_signature(
    sigs: &Signatures,
    layouts: &Layouts,
    sig: &FnSig,
    reified: bool,
) -> Signature {
    let convention = if reified {
        Convention::RustReified
    } else {
        Convention::RustFast
    };
    convention.signature(sigs, layouts, sig, false)
}

/// How a function returning `ret` returns it: nothing, one value, or
/// through memory with this layout.
pub(super) enum Return {
    Values(Vec<AbiParam>),
    Sret(Layout),
    /// A value of a memory type, laid out as it is.
    Memory(Layout),
}

fn return_kind(sigs: &Signatures, layouts: &Layouts, ret: Ty, fast: bool) -> Return {
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
    if let Some(returns) = register_results(&leaves, fast) {
        return Return::Values(returns);
    }
    let fields = leaves
        .iter()
        .map(|&leaf| Layout::Scalar(keel_type(leaf).expect("leaves are scalars")))
        .collect();
    Return::Sret(Layout::Struct(fields))
}

/// Several leaves returned as keel results, when the convention gives each
/// one its own register on both arm64 and x86-64. `cc(fast)` places results
/// as it places arguments: at most seven integers or pointers (x86-64 has
/// seven such registers) and eight floats. The C convention, which reified
/// and separately compiled functions use, returns them where C returns a
/// struct of the leaves, so it takes two integers or pointers, one of them
/// 8 bytes so that neither shares a register, or two `f64`. Anything else goes through
/// `sret`. Results carry no extension, as struct fields have none.
fn register_results(leaves: &[Ty], fast: bool) -> Option<Vec<AbiParam>> {
    let types = leaves
        .iter()
        .map(|&leaf| keel_type(leaf))
        .collect::<Option<Vec<_>>>()?;
    let is_int = |ty: &Type| matches!(ty, Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr);
    let is_float = |ty: &Type| matches!(ty, Type::F32 | Type::F64);
    let ints = types.iter().filter(|ty| is_int(ty)).count();
    let floats = types.iter().filter(|ty| is_float(ty)).count();
    if ints + floats != types.len() {
        return None;
    }
    let fits = if fast {
        ints <= 7 && floats <= 8
    } else {
        match types[..] {
            [a, b] if ints == 2 => [a, b].iter().any(|ty| matches!(ty, Type::I64 | Type::Ptr)),
            [Type::F64, Type::F64] => true,
            _ => false,
        }
    };
    fits.then(|| types.into_iter().map(AbiParam::new).collect())
}

/// Bound Rust aggregate expansion at call boundaries. In particular, an
/// enum's SSA leaves include every variant even though only one is stored.
/// Larger values cross calls through their actual layout, with the callee
/// loading its own by-value snapshot. Local scalar storage stays unchanged.
fn indirect_argument(sigs: &Signatures, ty: Ty) -> bool {
    !is_memory(sigs, ty) && leaf_count(sigs, ty) > 16
}
