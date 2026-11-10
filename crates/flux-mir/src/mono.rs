//! Monomorphization: the instances of functions a program needs, each
//! a function with its generic arguments, and their bodies with those
//! arguments substituted.
//!
//! A generic function's MIR is built and checked once, in terms of its
//! parameters; code is generated once per instance. The instances are
//! found from the functions that are not generic, through their calls
//! and the `Drop` impls their drops run, as rustc's collector does, so a
//! generic function nothing calls has none.
//!
//! A drop of a value is expanded here, once its type is known, into its
//! *drop glue*: calls of `Drop::drop` for each part of it whose type has
//! a `Drop` impl, the outer before the inner and fields in order, and
//! for an enum, of the variant it holds.

use std::borrow::Cow;

use rustc_hash::{FxHashMap, FxHashSet};

use flux_hir::{Diagnostic, FunctionId, Hir, ItemTree, StructId};
use flux_intern::Symbol;
pub use flux_ty::Instance;
use flux_ty::{GenericArgs, Mutability, Signatures, Ty, TyKind};

use crate::{
    AssertMessage, BasicBlock, BasicBlockData, Body, Constant, FileMir, Local, LocalDecl,
    LocalKind, Operand, Place, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};

/// rustc's `recursion_limit`: how deep a chain of instances, each
/// found in the one before, may be.
const RECURSION_LIMIT: u32 = 128;

/// The owning call uses an ordinary sized helper, or the object's owning slot.
fn box_call_once(items: &ItemTree, args: GenericArgs) -> Instance {
    let [tuple, payload] = *args else {
        unreachable!("boxed call arguments")
    };
    if payload.is_dyn() {
        let trait_ =
            flux_ty::fn_trait(items, flux_ty::ClosureKind::FnOnce).expect("embedded FnOnce trait");
        let function = items.traits[trait_]
            .function(items, Symbol::intern("call_once"))
            .expect("embedded FnOnce method");
        Instance {
            function,
            args: Ty::list(&[payload, tuple]),
        }
    } else {
        Instance {
            function: items.lang_fns[&Symbol::intern("box_call_once")],
            args,
        }
    }
}

/// Public immediate wrappers remain ordinary functions, including when reified.
/// Check each collected specialization rather than relying on a direct call edge.
fn neon_immediate_error(items: &ItemTree, instance: Instance) -> Option<&'static str> {
    let function = &items.functions[instance.function];
    if function.intrinsic == Some(flux_hir::Intrinsic::CoroutineAwait) {
        return Some("await reached code generation outside an async state machine");
    }
    if !items.is_core(function.loc.module)
        || items.modules[function.loc.module].name.as_str() != "aarch64"
    {
        return None;
    }
    let (lower, upper, message) = match function.name.as_str() {
        "vsetq_lane_u64" => (0, 2, "lane index out of range"),
        "vshll_n_u16" => (0, 17, "shift amount out of range: expected 0..=16"),
        "vshr_n_u32" => (1, 33, "shift amount out of range: expected 1..=32"),
        "vshrq_n_u32" => (1, 33, "shift amount out of range: expected 1..=32"),
        "vshlq_n_u32" => (0, 32, "shift amount out of range: expected 0..=31"),
        "vshrq_n_u8" => (1, 9, "shift amount out of range: expected 1..=8"),
        "vextq_u8" => (0, 16, "lane index out of range"),
        "vextq_u32" => (0, 4, "lane index out of range"),
        _ => return None,
    };
    (!matches!(instance.args.first().map(|ty|ty.kind()),Some(TyKind::Value(v)) if (lower..upper).contains(&v.bits())))
        .then_some(message)
}

/// Every instance of a function with a body that the program needs, in
/// the order they were found; or an error if a generic function needs
/// instances of itself without end. A call of a trait's method is of
/// the `impl` block's method for its types. As rustc does for a program,
/// they are found from `main`: other functions nothing calls are not
/// built, nor are the instances only they need.
pub fn collect(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Vec<Instance>, Diagnostic> {
    collect_all(hir, sigs, mir).map(|c| c.instances)
}

/// The instances a program needs, and those of them whose address it
/// takes ([`Constant::Fn`]), which must be callable through a pointer.
#[derive(Debug, Default)]
pub struct Collected {
    pub instances: Vec<Instance>,
    pub reified: FxHashSet<Instance>,
    /// Constants evaluated after their reachable caller supplies concrete arguments.
    pub const_values: FxHashMap<flux_ty::ConstInstance, crate::eval::Value>,
}

/// [`collect`], with the instances whose address is taken.
pub fn collect_all(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, None, None, &[], None)
}

/// Collects executable roots and additional concrete instances in one traversal.
/// Main, runtime initialization, termination, stdout flush and static roots remain active.
pub fn collect_all_with_roots(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    additional_roots: &[Instance],
) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, None, None, additional_roots, None)
}

/// Collects instances reachable from explicit roots, without an executable entry point.
pub fn collect_from_roots(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    roots: &[Instance],
) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, Some(roots), None, &[], None)
}

/// Collects explicit roots and only the selected crates' static initializers.
/// Other crates' statics belong to their own separately compiled objects.
pub fn collect_from_roots_in_crates(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    roots: &[Instance],
    static_crates: &[flux_hir::CrateId],
) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, Some(roots), Some(static_crates), &[], None)
}

/// Collects executable roots with an explicit set of retained static definitions.
pub fn collect_all_with_roots_and_statics(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    additional_roots: &[Instance],
    statics: &[flux_hir::ConstId],
) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, None, None, additional_roots, Some(statics))
}

/// Collects library instances and only its retained static definitions.
pub fn collect_from_roots_and_statics(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    roots: &[Instance],
    statics: &[flux_hir::ConstId],
) -> Result<Collected, Diagnostic> {
    collect_roots(hir, sigs, mir, Some(roots), None, &[], Some(statics))
}

fn collect_roots(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    roots: Option<&[Instance]>,
    static_crates: Option<&[flux_hir::CrateId]>,
    additional_roots: &[Instance],
    static_items: Option<&[flux_hir::ConstId]>,
) -> Result<Collected, Diagnostic> {
    let mut reified = FxHashSet::default();
    let mut instances = Vec::new();
    let mut depths: FxHashMap<Instance, u32> = FxHashMap::default();
    let items = &hir.items;
    let root = items.module_of(items.main_crate());
    let main = mir.bodies.iter().map(|(id, _)| id).find(|&id| {
        let f = &items.functions[id];
        f.name.as_str() == "main" && f.loc.module == root && f.owner.is_none()
    });
    if let Some(roots) = roots {
        for &instance in roots {
            if depths.insert(instance, 0).is_none() {
                instances.push(instance);
            }
        }
    } else if let Some(function) = main {
        let instance = Instance {
            function,
            args: &[],
        };
        depths.insert(instance, 0);
        instances.push(instance);
        let ret = flux_solve::traits::reveal(sigs, sigs[function].ret);
        if ret != Ty::UNIT
            && ret != Ty::NEVER
            && let Some(runner) = flux_ty::entry::main_runner(items, sigs)
        {
            reified.insert(instance);
            let instance = Instance {
                function: runner,
                args: Ty::list(&[ret]),
            };
            depths.insert(instance, 0);
            instances.push(instance);
        }
        // Runtime hooks invoked around the user main function.
        for hook in ["args_init", "flush_stdout"] {
            if let Some(&function) = items.lang_fns.get(&Symbol::intern(hook))
                && mir.bodies.get(function).is_some()
            {
                let instance = Instance {
                    function,
                    args: &[],
                };
                depths.insert(instance, 0);
                instances.push(instance);
            }
        }
    }
    if roots.is_none() {
        for (function, data) in items.functions.iter() {
            if !data.is_extern && data.link_name.is_some() && mir.bodies.get(function).is_some() {
                let instance = Instance {
                    function,
                    args: &[],
                };
                if depths.insert(instance, 0).is_none() {
                    instances.push(instance);
                }
            }
        }
    }
    for &instance in additional_roots {
        if depths.insert(instance, 0).is_none() {
            instances.push(instance);
        }
    }
    // Static values and their vtables are fixed for this collection. Cache the
    // complete traversal result, including empty answers, but still process
    // reification and recursion depth separately for every use below.
    let mut static_functions: FxHashMap<flux_hir::ConstId, Vec<Instance>> = FxHashMap::default();
    // All statics are emitted, but an ordinary constant becomes a root only
    // when reachable code uses it. Otherwise an unused pointer could trigger
    // unbounded monomorphization of a function that is never called.
    for (id, value) in mir.const_values.items.iter() {
        if items.consts[id].is_static.is_none()
            || static_crates.is_some_and(|crates| {
                !crates.contains(&items.modules[items.consts[id].loc.module].krate)
            })
            || static_items.is_some_and(|items| !items.contains(&id))
        {
            continue;
        }
        let mut found = Vec::new();
        value_functions(value, &mir.const_values, items, sigs, &mut found);
        static_functions.insert(id, found.clone());
        for instance in found {
            reified.insert(instance);
            if (mir.instance_body(items, instance).is_some() || is_builtin_shim(items, instance))
                && !depths.contains_key(&instance)
            {
                depths.insert(instance, 0);
                instances.push(instance);
            }
        }
    }
    let names = |id| items.const_name(id);
    let mut evaluator = crate::eval::Evaluator::new(items, sigs, &mir.bodies, &mir.consts, &names)
        .with_imported(&mir.const_values, &mir.discriminants);
    let mut const_values = FxHashMap::default();
    let mut next = 0;
    while let Some(&caller) = instances.get(next) {
        next += 1;
        if let Some(message) = neon_immediate_error(items, caller) {
            let function = &items.functions[caller.function];
            let mut error = Diagnostic::new(function.def_range, message);
            error.file = function.loc.file;
            return Err(error);
        }
        let depth = depths[&caller];
        let shim;
        let (body, caller_args) = if let Some(generated) =
            crate::coroutine::body_of(hir, sigs, mir, caller)
        {
            shim = generated;
            (&shim, &[][..])
        } else {
            match mir.instance_body(&hir.items, caller) {
                Some(found) => found,
                None => {
                    shim = builtin_shim(&hir.items, sigs, caller).expect("instances have bodies");
                    (&shim, caller.args)
                }
            }
        };
        // A generic wrapper can acquire a vtable tail only at this instance.
        // Reject that unrepresented dynamic layout before intrinsic expansion.
        let mut types: Vec<_> = body
            .locals
            .values()
            .map(|local| flux_solve::traits::instance_ty(items, sigs, local.ty, caller_args))
            .collect();
        let mut seen = FxHashSet::default();
        while let Some(ty) = types.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if matches!(ty.kind(), TyKind::Adt(..))
                && sigs.metadata(ty) == flux_ty::Metadata::VTable
                && flux_solve::transparent_object_tail(items, &sigs.structs, ty).is_none()
            {
                let owner = &items.functions[body.owner.unwrap_or(caller.function)];
                let mut error = Diagnostic::new(
                    owner.def_range,
                    "trait object tails in structs are not supported yet",
                );
                error.file = owner.loc.file;
                return Err(error);
            }
            types.extend(ty.children());
        }
        for data in body.blocks.values() {
            // Functions whose address is taken.
            for statement in &data.statements {
                // A vtable's methods, for a value made a trait object.
                if let StatementKind::Assign(_, Rvalue::Unsize(op, to)) = &statement.kind {
                    let from = flux_solve::traits::instance_ty(
                        &hir.items,
                        sigs,
                        body.operand_ty(op, sigs),
                        caller_args,
                    );
                    let to = flux_solve::traits::instance_ty(&hir.items, sigs, *to, caller_args);
                    let mut vtables = Vec::new();
                    if let (Some(concrete), Some(object)) =
                        (pointee(items, from), pointee(items, to))
                        && let Some((concrete, object)) = flux_solve::transparent_object_unsize(
                            items,
                            &sigs.structs,
                            concrete,
                            object,
                        )
                        && object.is_dyn()
                        && !concrete.is_dyn()
                    {
                        vtables.push((object, concrete));
                    }
                    while let Some((object, concrete)) = vtables.pop() {
                        let drop = crate::vtable::drop_entry(&hir.items, sigs, concrete)
                            .map(crate::vtable::Entry::Method);
                        let entries = crate::vtable::entries(&hir.items, sigs, object, concrete);
                        for entry in drop.into_iter().chain(entries) {
                            match entry {
                                crate::vtable::Entry::Method(instance) => {
                                    reified.insert(instance);
                                    let has_body =
                                        mir.instance_body(&hir.items, instance).is_some()
                                            || is_builtin_shim(&hir.items, instance);
                                    if has_body && !depths.contains_key(&instance) {
                                        depths.insert(instance, depth + 1);
                                        instances.push(instance);
                                    }
                                }
                                crate::vtable::Entry::Super(sup) => {
                                    vtables.push((sup, concrete));
                                }
                                crate::vtable::Entry::Null => {}
                            }
                        }
                    }
                }
            }
            let pointers = data
                .statements
                .iter()
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign(_, rvalue) => Some(rvalue),
                    _ => None,
                })
                .flat_map(Rvalue::operands)
                .chain(data.terminator.kind.operands());
            for op in pointers {
                let mut found = Vec::new();
                match op {
                    Operand::Const(Constant::Fn { function, args, .. }) => {
                        let instance = Instance {
                            function: *function,
                            args: instance_args(items, sigs, args, caller_args),
                        };
                        found.push(flux_solve::traits::resolve_instance(items, sigs, instance));
                    }
                    Operand::Const(Constant::Static { id, .. }) => {
                        if let Some(known) = static_functions.get(id) {
                            found.extend_from_slice(known);
                        } else if let Some(value) = mir.const_values.items.get(*id) {
                            value_functions(value, &mir.const_values, items, sigs, &mut found);
                            static_functions.insert(*id, found.clone());
                        }
                    }
                    Operand::Const(
                        Constant::Item { id, args, ty } | Constant::ItemRef { id, args, ty },
                    ) => {
                        let ty = flux_solve::traits::instance_ty(items, sigs, *ty, caller_args);
                        if !may_contain_function_address(items, sigs, ty) {
                            continue;
                        }
                        let c = flux_ty::ConstInstance {
                            konst: *id,
                            args: instance_args(items, sigs, args, caller_args),
                        };
                        let c = flux_solve::traits::resolve_const(items, sigs, c);
                        let value = if let Some(value) = mir.const_values.get(c.konst, c.args) {
                            value
                        } else {
                            if let std::collections::hash_map::Entry::Vacant(entry) =
                                const_values.entry(c)
                            {
                                let value = evaluator.eval_instance(c).map_err(|error| {
                                    let mut d = Diagnostic::new(error.range, error.message);
                                    d.file = items.consts[c.konst].loc.file;
                                    d
                                })?;
                                entry.insert(value);
                            }
                            &const_values[&c]
                        };
                        value_functions(value, &mir.const_values, items, sigs, &mut found);
                    }
                    Operand::Const(Constant::Promoted { index, .. }) => {
                        let original = &body.promoted[*index as usize];
                        let value = if caller_args.is_empty() {
                            Cow::Borrowed(original)
                        } else {
                            Cow::Owned(original.map_functions(&|instance| {
                                flux_solve::traits::resolve_instance(
                                    items,
                                    sigs,
                                    Instance {
                                        function: instance.function,
                                        args: instance_args(
                                            items,
                                            sigs,
                                            instance.args,
                                            caller_args,
                                        ),
                                    },
                                )
                            }))
                        };
                        value_functions(&value, &mir.const_values, items, sigs, &mut found);
                    }
                    _ => {}
                }
                for instance in found {
                    reified.insert(instance);
                    let has_body = mir.instance_body(items, instance).is_some()
                        || is_builtin_shim(items, instance);
                    if has_body && !depths.contains_key(&instance) {
                        if depth + 1 > RECURSION_LIMIT {
                            let mut d = Diagnostic::new(
                                data.terminator.range,
                                format!(
                                    "reached the recursion limit while instantiating `{}`",
                                    instance_name(hir, instance),
                                ),
                            );
                            d.file = items.functions[caller.function].loc.file;
                            return Err(d);
                        }
                        depths.insert(instance, depth + 1);
                        instances.push(instance);
                    }
                }
            }
            let single;
            let callees: Cow<'_, [Instance]> = match &data.terminator.kind {
                TerminatorKind::Call {
                    func, generic_args, ..
                } if items.functions[*func].intrinsic == Some(flux_hir::Intrinsic::BoxCallOnce) => {
                    single =
                        box_call_once(items, instance_args(items, sigs, generic_args, caller_args));
                    Cow::Borrowed(std::slice::from_ref(&single))
                }
                TerminatorKind::Call {
                    func, generic_args, ..
                } if items.functions[*func].intrinsic
                    == Some(flux_hir::Intrinsic::SizedDispatch) =>
                {
                    let args = instance_args(items, sigs, generic_args, caller_args);
                    let function =
                        flux_ty::sized_dispatch::select(sigs, *func, args).map_err(|message| {
                            let mut error = Diagnostic::new(data.terminator.range, message);
                            error.file = items.functions[caller.function].loc.file;
                            error
                        })?;
                    single = Instance { function, args };
                    Cow::Borrowed(std::slice::from_ref(&single))
                }
                // What dropping in place drops.
                TerminatorKind::Call {
                    func, generic_args, ..
                } if hir.items.functions[*func].intrinsic.is_some()
                    && hir.items.functions[*func].name.as_str() == "drop_in_place" =>
                {
                    let ty = instance_args(&hir.items, sigs, generic_args, caller_args)
                        .first()
                        .copied()
                        .unwrap_or(Ty::ERROR);
                    let elem = match ty.kind() {
                        TyKind::Slice(elem) => elem,
                        _ => ty,
                    };
                    Cow::Owned(drop_instances(&hir.items, sigs, elem))
                }
                TerminatorKind::Call {
                    func, generic_args, ..
                } => {
                    single = Instance {
                        function: *func,
                        args: instance_args(&hir.items, sigs, generic_args, caller_args),
                    };
                    Cow::Borrowed(std::slice::from_ref(&single))
                }
                // What a failed check calls.
                TerminatorKind::Assert { msg, .. } => {
                    if let Some(&function) = items.lang_fns.get(&Symbol::intern(msg.lang_fn())) {
                        single = Instance {
                            function,
                            args: &[],
                        };
                        Cow::Borrowed(std::slice::from_ref(&single))
                    } else {
                        Cow::Borrowed(&[])
                    }
                }
                TerminatorKind::Drop { place, .. } => {
                    let ty = body.place_ty(place, sigs);
                    let ty = flux_solve::traits::instance_ty(&hir.items, sigs, ty, caller_args);
                    let mut glue = Vec::new();
                    drop_glue(&hir.items, sigs, place.clone(), ty, &mut glue);
                    let mut types = Vec::new();
                    glue_types(&glue, &mut types);
                    let drop = drop_fn(&hir.items);
                    Cow::Owned(
                        types
                            .into_iter()
                            .filter_map(|ty| {
                                Some(Instance {
                                    function: drop?,
                                    args: Ty::list(&[ty]),
                                })
                            })
                            .collect(),
                    )
                }
                _ => continue,
            };
            for &callee in callees.iter() {
                let callee = flux_solve::traits::resolve_instance(&hir.items, sigs, callee);
                // Through a vtable, whose methods are collected where
                // values become trait objects.
                if crate::vtable::virtual_call(&hir.items, callee).is_some() {
                    continue;
                }
                let callee_data = &hir.items.functions[callee.function];
                let name = callee_data.name;
                if hir.items.is_core(callee_data.loc.module)
                    && (name.as_str().starts_with("_mm") || name.as_str().starts_with("__x86_mm"))
                    && !callee.args.is_empty()
                {
                    let limit = if matches!(
                        name.as_str(),
                        "_mm256_extracti128_si256" | "_mm256_insertf128_si256"
                    ) {
                        2
                    } else if matches!(
                        name.as_str(),
                        "_mm_extract_epi32"
                            | "__x86_mm_extract_epi32"
                            | "_mm512_extracti32x4_epi32"
                            | "_mm_prefetch"
                            | "__x86_mm_prefetch"
                    ) {
                        4
                    } else {
                        256
                    };
                    if !matches!(callee.args.first().map(|ty| ty.kind()), Some(TyKind::Value(v)) if v.bits() < limit)
                    {
                        let mut d =
                            Diagnostic::new(data.terminator.range, "immediate out of range");
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
                    }
                }

                if hir.items.is_core(callee_data.loc.module)
                    && matches!(
                        name.as_str(),
                        "vgetq_lane_u64"
                            | "vget_lane_u64"
                            | "__neon_vgetq_lane_u64"
                            | "__neon_vget_lane_u64"
                    )
                {
                    let limit =
                        if matches!(name.as_str(), "vgetq_lane_u64" | "__neon_vgetq_lane_u64") {
                            2
                        } else {
                            1
                        };
                    if !matches!(callee.args.first().map(|ty| ty.kind()), Some(TyKind::Value(v)) if v.bits() < limit)
                    {
                        let mut d =
                            Diagnostic::new(data.terminator.range, "lane index out of range");
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
                    }
                }

                if hir.items.functions[callee.function]
                    .intrinsic
                    .is_some_and(|intrinsic| !intrinsic.is_coroutine())
                {
                    if let Some((op, _, word)) =
                        atomic_intrinsic(hir.items.functions[callee.function].name.as_str())
                    {
                        let sig = flux_solve::traits::instance_sig(
                            &hir.items,
                            sigs,
                            callee.function,
                            callee.args,
                        );
                        if !valid_atomic_signature(op, word, callee.args, &sig) {
                            let mut d = Diagnostic::new(
                                data.terminator.range,
                                if word == AtomicWord::Ptr {
                                    "atomic intrinsic requires naturally aligned thin pointer storage and an exact pointer signature"
                                } else if word == AtomicWord::U8 {
                                    "atomic intrinsic requires u8 storage and an exact scalar signature"
                                } else if word == AtomicWord::U16 {
                                    "atomic intrinsic requires u16 storage and an exact scalar signature"
                                } else if word == AtomicWord::U32 {
                                    "atomic intrinsic requires naturally aligned u32 storage and an exact scalar signature"
                                } else {
                                    "atomic intrinsic requires naturally aligned usize storage and an exact scalar signature"
                                },
                            );
                            d.file = hir.items.functions[caller.function].loc.file;
                            return Err(d);
                        }
                    }
                    if hir.items.functions[callee.function].name.as_str() == "read_unaligned"
                        && !matches!(
                            callee.args.first().map(|ty| ty.kind()),
                            Some(TyKind::Int(
                                flux_ty::IntTy::U8
                                    | flux_ty::IntTy::U16
                                    | flux_ty::IntTy::U32
                                    | flux_ty::IntTy::U64
                                    | flux_ty::IntTy::U128
                                    | flux_ty::IntTy::Usize
                            ))
                        )
                    {
                        let mut d = Diagnostic::new(
                            data.terminator.range,
                            "read_unaligned supports only unsigned integers yet",
                        );
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
                    }
                    if matches!(
                        hir.items.functions[callee.function].intrinsic,
                        Some(
                            flux_hir::Intrinsic::ReadVolatile | flux_hir::Intrinsic::WriteVolatile
                        )
                    ) {
                        let ty = callee.args.first().copied().unwrap_or(Ty::ERROR);
                        let layouts =
                            crate::layout::Layouts::new(&hir.items, sigs, &mir.discriminants);
                        let supported = (layouts.of(ty).exact && layouts.of(ty).size <= 32)
                            || volatile_scalar(sigs, ty).is_some_and(|scalar| {
                                let layout = layouts.of(ty);
                                let scalar_layout = layouts.of(scalar);
                                layout.size == scalar_layout.size
                                    && layout.align == scalar_layout.align
                            });
                        if !supported {
                            let mut d = Diagnostic::new(
                                data.terminator.range,
                                "volatile operations support only scalars or aggregates up to 32 bytes yet",
                            );
                            d.file = hir.items.functions[caller.function].loc.file;
                            return Err(d);
                        }
                    }
                    // A size that would not be rustc's.
                    let name = hir.items.functions[callee.function].name;
                    if matches!(name.as_str(), "size_of" | "align_of")
                        && let Some(&ty) = callee.args.first()
                        && !crate::layout::Layouts::new(&hir.items, sigs, &mir.discriminants)
                            .of(ty)
                            .exact
                    {
                        let mut d = Diagnostic::new(
                            data.terminator.range,
                            format!("`{name}` of `{ty}` is not supported yet"),
                        );
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
                    }
                    if name.as_str() == "__neon_vshrn_n_u16"
                        && !matches!(callee.args.first().map(|ty| ty.kind()), Some(TyKind::Value(v)) if (1..=8).contains(&v.bits()))
                    {
                        let mut d = Diagnostic::new(
                            data.terminator.range,
                            "shift amount out of range: expected 1..=8",
                        );
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
                    }
                    continue;
                }
                if mir.instance_body(&hir.items, callee).is_none()
                    && !is_builtin_shim(&hir.items, callee)
                {
                    continue;
                }
                if depths.contains_key(&callee) {
                    continue;
                }
                if depth + 1 > RECURSION_LIMIT {
                    let mut d = Diagnostic::new(
                        data.terminator.range,
                        format!(
                            "reached the recursion limit while instantiating `{}`",
                            instance_name(hir, callee)
                        ),
                    );
                    d.file = hir.items.functions[caller.function].loc.file;
                    return Err(d);
                }
                depths.insert(callee, depth + 1);
                instances.push(callee);
            }
        }
    }
    // Code generation needs concrete types: a type that is an error or
    // a parameter here is a bug in flux, which must not reach it.
    let abstract_ty = |a: &Ty| a.has_error() || a.has_params() || a.has_infer();
    if let Some(bad) = instances.iter().find(|i| i.args.iter().any(abstract_ty)) {
        let f = &hir.items.functions[bad.function];
        let message = format!(
            "internal compiler error: an instance of `{}` with generic arguments that are not concrete types",
            items.function_path(f)
        );
        if cfg!(debug_assertions) {
            panic!("{message}");
        }
        let mut d = Diagnostic::new(f.def_range, message);
        d.file = f.loc.file;
        return Err(d);
    }
    Ok(Collected {
        instances,
        reified,
        const_values,
    })
}

/// Whether evaluating this stored type can reveal a function address. Scalar
/// constants stay in codegen's evaluator, outside the collector's large frame.
fn may_contain_function_address(items: &ItemTree, sigs: &Signatures, ty: Ty) -> bool {
    let mut pending = vec![(ty, Vec::new())];
    while let Some((ty, mut active)) = pending.pop() {
        let ty = flux_solve::traits::normalize(items, sigs, &[], ty);
        match ty.kind() {
            TyKind::FnPtr(_) | TyKind::Dynamic(_) => return true,
            TyKind::Ref(_, inner)
            | TyKind::RawPtr(_, inner)
            | TyKind::Array(inner, _)
            | TyKind::Slice(inner) => pending.push((inner, active)),
            TyKind::Adt(adt, args) => {
                // A repeated declaration can introduce function pointers via
                // associated fields after substitution. Evaluate conservatively
                // rather than unfold pointer-linked growing types forever.
                if active.contains(&adt.strukt) {
                    return true;
                }
                active.push(adt.strukt);
                let children = flux_solve::contents(sigs, ty).unwrap_or(args);
                pending.extend(children.iter().map(|&child| (child, active.clone())));
            }
            TyKind::Tuple(fields) => {
                pending.extend(fields.iter().map(|&child| (child, active.clone())));
            }
            TyKind::Closure(_, _) => {
                pending.extend(
                    flux_solve::contents(sigs, ty)
                        .unwrap_or_default()
                        .iter()
                        .map(|&child| (child, active.clone())),
                );
            }
            _ => {}
        }
    }
    false
}

/// Function addresses nested in a constant's stored value.
pub(crate) fn value_functions<'a>(
    value: &'a crate::eval::Value,
    constants: &'a crate::eval::ConstValues,
    items: &ItemTree,
    sigs: &Signatures,
    out: &mut Vec<Instance>,
) {
    use crate::eval::Value;
    let mut values = vec![value];
    let mut globals = FxHashSet::default();
    let mut allocations = FxHashSet::default();
    while let Some(value) = values.pop() {
        match value {
            Value::Function(instance) => out.push(*instance),
            Value::DynPointer {
                pointer,
                concrete,
                object,
                ..
            } => {
                let mut pending = vec![*object];
                let mut seen = FxHashSet::default();
                while let Some(object) = pending.pop() {
                    if !seen.insert(object) {
                        continue;
                    }
                    if let Some(drop) = crate::vtable::drop_entry(items, sigs, *concrete) {
                        out.push(drop);
                    }
                    for entry in crate::vtable::entries(items, sigs, object, *concrete) {
                        match entry {
                            crate::vtable::Entry::Method(instance) => out.push(instance),
                            crate::vtable::Entry::Super(object) => pending.push(object),
                            crate::vtable::Entry::Null => {}
                        }
                    }
                }
                if allocations.insert((0, std::sync::Arc::as_ptr(pointer) as usize)) {
                    values.push(pointer);
                }
            }
            Value::Aggregate(fields) | Value::Variant { fields, .. } => {
                if allocations.insert((1, std::sync::Arc::as_ptr(fields) as *const Value as usize))
                {
                    values.extend(fields.iter());
                }
            }
            Value::Ref(value) | Value::InteriorRef(value) | Value::LocalRef { value, .. }
                if allocations.insert((0, std::sync::Arc::as_ptr(value) as usize)) =>
            {
                values.push(value);
            }
            Value::PointerView { pointer, .. } | Value::PointerOffset { pointer, .. }
                if allocations.insert((0, std::sync::Arc::as_ptr(pointer) as usize)) =>
            {
                values.push(pointer);
            }
            Value::Static(id) | Value::StaticPart { id, .. } if globals.insert(*id) => {
                if let Some(value) = constants.items.get(*id) {
                    values.push(value);
                }
            }
            _ => {}
        }
    }
}

/// What a pointer type points to.
fn pointee(items: &ItemTree, ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => Some(t),
        _ => flux_solve::box_pointee(ty).or_else(|| flux_ty::shared_pointee(items, ty)),
    }
}

/// The `Drop::drop` instances dropping a `ty` calls.
fn drop_instances(items: &ItemTree, sigs: &Signatures, ty: Ty) -> Vec<Instance> {
    let mut glue = Vec::new();
    drop_glue(items, sigs, Place::from(Local::from_raw(0)), ty, &mut glue);
    let mut types = Vec::new();
    glue_types(&glue, &mut types);
    let drop = drop_fn(items);
    types
        .into_iter()
        .filter_map(|ty| {
            Some(Instance {
                function: drop?,
                args: Ty::list(&[ty]),
            })
        })
        .collect()
}

/// `args`, in terms of a caller's parameters, for its instance with
/// `caller_args`: substituted, with opaque types revealed and associated
/// types normalized, as [`subst_body`] makes them.
fn instance_args(
    items: &ItemTree,
    sigs: &Signatures,
    args: &[Ty],
    caller_args: &[Ty],
) -> GenericArgs {
    if caller_args.is_empty() && !args.iter().any(|a| a.has_opaques()) {
        return Ty::list(args);
    }
    let list: Vec<Ty> = args
        .iter()
        .map(|&a| flux_solve::traits::instance_ty(items, sigs, a, caller_args))
        .collect();
    Ty::list(&list)
}

/// Whether `body` mentions opaque types, which its instances reveal.
fn mentions_opaques(body: &Body) -> bool {
    body.locals.values().any(|l| l.ty.has_opaques())
        || body
            .blocks
            .values()
            .any(|data| match &data.terminator.kind {
                TerminatorKind::Call { generic_args, .. } => {
                    generic_args.iter().any(|a| a.has_opaques())
                }
                _ => false,
            })
}

/// The MIR of `instance`: its function's with its generic arguments
/// substituted and its drops expanded, or a compiler-supplied callable method.
pub fn body_of<'b>(
    hir: &Hir,
    sigs: &Signatures,
    mir: &'b FileMir,
    instance: Instance,
) -> Cow<'b, Body> {
    if let Some(generated) = crate::coroutine::body_of(hir, sigs, mir, instance) {
        return Cow::Owned(
            instance_body(hir, sigs, &mir.discriminants, &generated, &[]).into_owned(),
        );
    }
    match mir.instance_body(&hir.items, instance) {
        Some((body, args)) => instance_body(hir, sigs, &mir.discriminants, body, args),
        None => {
            Cow::Owned(builtin_shim(&hir.items, sigs, instance).expect("instances have bodies"))
        }
    }
}

/// The method of trait `lang` (a lang item's name) called `name`.
fn lang_method(items: &ItemTree, lang: &str, name: &str) -> Option<FunctionId> {
    let t = *items.lang_traits.get(&flux_intern::Symbol::intern(lang))?;
    items.traits[t].function(items, flux_intern::Symbol::intern(name))
}

/// Whether a callable's copy, call or address-formatting method has a
/// compiler-supplied body instead of an ordinary library impl.
fn is_builtin_shim(items: &ItemTree, instance: Instance) -> bool {
    if is_fn_shim(items, instance)
        || is_pointer_debug_shim(items, instance)
        || is_pointer_address_shim(items, instance)
    {
        return true;
    }
    // Function values and closures have compiler-supplied Clone bodies.
    if matches!(
        instance.args.first().map(|t| t.kind()),
        Some(TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_))
    ) && Some(instance.function) == lang_method(items, "clone", "clone")
    {
        return true;
    }
    false
}

/// Clone function values by copying and closures by cloning their captures.
fn builtin_shim(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Option<Body> {
    if !is_builtin_shim(items, instance) {
        return None;
    }
    if is_fn_shim(items, instance) {
        return Some(fn_shim(items, sigs, instance));
    }
    if is_pointer_debug_shim(items, instance) {
        return Some(pointer_debug_shim(items, sigs, instance));
    }
    if is_pointer_address_shim(items, instance) {
        return Some(pointer_address_shim(items, sigs, instance));
    }
    let ty = instance.args[0];
    let mut locals = flux_arena::Arena::new();
    let ret = locals.alloc(LocalDecl {
        ty,
        kind: LocalKind::Return,
    });
    let this = locals.alloc(LocalDecl {
        ty: Ty::new(TyKind::Ref(Mutability::Not, ty)),
        kind: LocalKind::Param,
    });
    let range = flux_syntax::TextRange::default();
    let mut blocks = flux_arena::Arena::new();
    if let TyKind::Closure(_, args) = ty.kind() {
        // Even a Copy capture may have an observable Clone implementation.
        for (index, &field) in flux_ty::ClosureArgs::split(args)
            .upvar_types()
            .iter()
            .enumerate()
        {
            let reference = locals.alloc(LocalDecl {
                ty: Ty::new(TyKind::Ref(Mutability::Not, field)),
                kind: LocalKind::Temp,
            });
            blocks.alloc(BasicBlockData {
                statements: vec![Statement {
                    kind: StatementKind::Assign(
                        reference.into(),
                        Rvalue::Ref {
                            raw: false,
                            mutability: Mutability::Not,
                            place: Place::from(this).deref().field(index as u32),
                            two_phase: false,
                        },
                    ),
                    range,
                }],
                terminator: Terminator {
                    kind: TerminatorKind::Call {
                        func: instance.function,
                        generic_args: Ty::list(&[field]),
                        args: vec![Operand::Move(reference.into())],
                        dest: Place::from(ret).field(index as u32),
                        target: Some(BasicBlock::from_raw(index as u32 + 1)),
                    },
                    range,
                },
            });
        }
        blocks.alloc(BasicBlockData {
            statements: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Return,
                range,
            },
        });
    } else {
        blocks.alloc(BasicBlockData {
            statements: vec![Statement {
                kind: StatementKind::Assign(
                    ret.into(),
                    Rvalue::Use(Operand::Copy(Place::from(this).deref())),
                ),
                range,
            }],
            terminator: Terminator {
                kind: TerminatorKind::Return,
                range,
            },
        });
    }
    Some(Body {
        locals,
        call_regions: flux_arena::ArenaMap::new(),
        reification_regions: flux_arena::ArenaMap::new(),
        region_annotations: Vec::new(),
        region_relations: Vec::new(),
        blocks,
        param_count: 1,
        c_strings: Vec::new(),
        in_core: true,
        inherits_overflow_checks: false,
        live_drops: Vec::new(),
        init_errors: Vec::new(),
        upvars: None,
        capture_uses: Vec::new(),
        owner: None,
        constant_owner: None,
        promoted: Vec::new(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    })
}

/// The private address method accepts only function pointers, not function items.
fn is_pointer_address_shim(items: &ItemTree, instance: Instance) -> bool {
    matches!(
        instance.args.first().map(|ty| ty.kind()),
        Some(TyKind::FnPtr(_))
    ) && Some(instance.function) == lang_method(items, "fn_ptr_trait", "addr")
}

fn pointer_address_shim(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Body {
    let signature = flux_solve::traits::instance_sig(items, sigs, instance.function, instance.args);
    let mut locals = flux_arena::Arena::new();
    let ret = locals.alloc(LocalDecl {
        ty: signature.ret,
        kind: LocalKind::Return,
    });
    let pointer = locals.alloc(LocalDecl {
        ty: signature.params[0],
        kind: LocalKind::Param,
    });
    let range = flux_syntax::TextRange::default();
    let mut blocks = flux_arena::Arena::new();
    blocks.alloc(BasicBlockData {
        statements: vec![Statement {
            kind: StatementKind::Assign(
                ret.into(),
                Rvalue::Cast(Operand::Copy(pointer.into()), signature.ret),
            ),
            range,
        }],
        terminator: Terminator {
            kind: TerminatorKind::Return,
            range,
        },
    });
    Body {
        locals,
        blocks,
        param_count: 1,
        call_regions: Default::default(),
        reification_regions: Default::default(),
        region_annotations: Vec::new(),
        region_relations: Vec::new(),
        c_strings: Vec::new(),
        in_core: true,
        inherits_overflow_checks: false,
        live_drops: Vec::new(),
        init_errors: Vec::new(),
        upvars: None,
        capture_uses: Vec::new(),
        owner: None,
        constant_owner: None,
        promoted: Vec::new(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    }
}

/// Pointer formatting observes only the address, never its signature values.
fn is_pointer_debug_shim(items: &ItemTree, instance: Instance) -> bool {
    matches!(
        instance.args.first().map(|ty| ty.kind()),
        Some(TyKind::FnPtr(_))
    ) && Some(instance.function) == lang_method(items, "Debug", "fmt")
}

fn pointer_debug_shim(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Body {
    let signature = flux_solve::traits::instance_sig(items, sigs, instance.function, instance.args);
    let helper = items
        .functions
        .iter()
        .find_map(|(id, f)| {
            (items.is_core(f.loc.module)
                && items.modules[f.loc.module].name.as_str() == "fmt"
                && f.owner.is_none()
                && f.name.as_str() == "pointer_fmt_inner")
                .then_some(id)
        })
        .expect("core pointer formatter");
    let mut locals = flux_arena::Arena::new();
    let ret = locals.alloc(LocalDecl {
        ty: signature.ret,
        kind: LocalKind::Return,
    });
    let this = locals.alloc(LocalDecl {
        ty: signature.params[0],
        kind: LocalKind::Param,
    });
    let formatter = locals.alloc(LocalDecl {
        ty: signature.params[1],
        kind: LocalKind::Param,
    });
    let address = locals.alloc(LocalDecl {
        ty: Ty::int(flux_ty::IntTy::Usize),
        kind: LocalKind::Temp,
    });
    let range = flux_syntax::TextRange::default();
    let mut blocks = flux_arena::Arena::new();
    blocks.alloc(BasicBlockData {
        statements: vec![Statement {
            kind: StatementKind::Assign(
                address.into(),
                Rvalue::Cast(
                    Operand::Copy(Place::from(this).deref()),
                    Ty::int(flux_ty::IntTy::Usize),
                ),
            ),
            range,
        }],
        terminator: Terminator {
            kind: TerminatorKind::Call {
                func: helper,
                generic_args: &[],
                args: vec![
                    Operand::Copy(address.into()),
                    Operand::Move(formatter.into()),
                ],
                dest: ret.into(),
                target: Some(BasicBlock::from_raw(1)),
            },
            range,
        },
    });
    blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Return,
            range,
        },
    });
    Body {
        locals,
        blocks,
        param_count: 2,
        call_regions: Default::default(),
        reification_regions: Default::default(),
        region_annotations: Vec::new(),
        region_relations: Vec::new(),
        c_strings: Vec::new(),
        in_core: true,
        inherits_overflow_checks: false,
        live_drops: Vec::new(),
        init_errors: Vec::new(),
        upvars: None,
        capture_uses: Vec::new(),
        owner: None,
        constant_owner: None,
        promoted: Vec::new(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    }
}

/// `PartialEq::eq` for arrays of `T` and `U`, with `instance`'s
/// arguments: `T::eq` of each pair of elements until one is false.
/// Whether `instance` is a `Fn`-family method of a function item or
/// pointer, which the compiler implements.
fn is_fn_shim(items: &ItemTree, instance: Instance) -> bool {
    crate::fn_method_kind(items, instance.function).is_some()
        && matches!(
            instance.args.first().map(|t| t.kind()),
            Some(TyKind::FnDef(_) | TyKind::FnPtr(_))
        )
}

/// `call`, `call_mut` or `call_once` of a function item or pointer: a
/// call of the function with the arguments taken out of their tuple.
fn fn_shim(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Body {
    let kind = crate::fn_method_kind(items, instance.function).expect("a Fn-family method");
    let callee_ty = instance.args[0];
    let tuple = instance.args.get(1).copied().unwrap_or(Ty::UNIT);
    let sig = flux_solve::traits::instance_sig(items, sigs, instance.function, instance.args);
    let mut locals = flux_arena::Arena::new();
    let ret = locals.alloc(LocalDecl {
        ty: sig.ret,
        kind: LocalKind::Return,
    });
    let this_ty = match kind {
        flux_ty::ClosureKind::Fn => Ty::new(TyKind::Ref(Mutability::Not, callee_ty)),
        flux_ty::ClosureKind::FnMut => Ty::new(TyKind::Ref(Mutability::Mut, callee_ty)),
        flux_ty::ClosureKind::FnOnce => callee_ty,
    };
    let this = locals.alloc(LocalDecl {
        ty: this_ty,
        kind: LocalKind::Param,
    });
    let args_local = locals.alloc(LocalDecl {
        ty: tuple,
        kind: LocalKind::Param,
    });
    let count = match tuple.kind() {
        TyKind::Tuple(list) => list.len(),
        _ => 0,
    };
    let args: Vec<Operand> = (0..count)
        .map(|i| Operand::Move(Place::from(args_local).field(i as u32)))
        .collect();
    let range = flux_syntax::TextRange::default();
    let target = Some(BasicBlock::from_raw(1));
    let call = match callee_ty.kind() {
        TyKind::FnDef(def) => TerminatorKind::Call {
            func: def.function,
            generic_args: def.args,
            args,
            dest: ret.into(),
            target,
        },
        _ => {
            let pointer = match kind {
                flux_ty::ClosureKind::FnOnce => Place::from(this),
                _ => Place::from(this).deref(),
            };
            TerminatorKind::CallIndirect {
                callee: Operand::Copy(pointer),
                args,
                dest: ret.into(),
                target,
            }
        }
    };
    let mut blocks = flux_arena::Arena::new();
    blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: Terminator { kind: call, range },
    });
    blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Return,
            range,
        },
    });
    Body {
        locals,
        call_regions: flux_arena::ArenaMap::new(),
        reification_regions: flux_arena::ArenaMap::new(),
        region_annotations: Vec::new(),
        region_relations: Vec::new(),
        blocks,
        param_count: 2,
        c_strings: Vec::new(),
        in_core: false,
        inherits_overflow_checks: false,
        live_drops: Vec::new(),
        init_errors: Vec::new(),
        upvars: None,
        capture_uses: Vec::new(),
        owner: None,
        constant_owner: None,
        promoted: Vec::new(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    }
}

/// The MIR of `instance`, of a function whose MIR is `body`: with its
/// generic arguments substituted and its drops expanded.
pub fn instance_body<'b>(
    hir: &Hir,
    sigs: &Signatures,
    discriminants: &crate::Discriminants,
    body: &'b Body,
    args: &[Ty],
) -> Cow<'b, Body> {
    let mut body = if args.is_empty() && !mentions_opaques(body) {
        Cow::Borrowed(body)
    } else {
        Cow::Owned(subst_body(hir, sigs, body, args))
    };
    // Intrinsics first: dropping in place is a drop to expand.
    let intrinsics = body.blocks.values().any(|data| {
        matches!(data.terminator.kind, TerminatorKind::Call { func, .. }
            if hir.items.functions[func].intrinsic.is_some())
    });
    if intrinsics {
        expand_intrinsics(&hir.items, sigs, discriminants, body.to_mut());
    }
    let drops = body
        .blocks
        .values()
        .any(|data| matches!(data.terminator.kind, TerminatorKind::Drop { .. }));
    if drops {
        expand_drops(&hir.items, sigs, body.to_mut());
    }
    body
}

/// Replaces each call of one of `core`'s intrinsics in `body`, whose
/// types are known, with what it does:
///
/// - `read(src)`: a copy of `*src`, which the caller will not drop;
/// - `write(dst, value)`: `*dst = value`, without dropping `*dst`;
/// - `uninit()`: nothing, its memory written before it is read;
/// - `is_copy::<T>()`: whether `T` is `Copy`;
/// - `size_of::<T>()`, `align_of::<T>()`: `T`'s size and alignment, as
///   rustc lays it out;
/// - `subslice(s, start, end)`, `subslice_mut(s, start, end)`:
///   `&s[start..end]`, unchecked;
/// - `slice_from_raw_parts(data, len)`: `data` as a pointer to an empty
///   array, unsized, then its first `len` elements, unchecked;
pub fn expand_intrinsics(
    items: &ItemTree,
    sigs: &Signatures,
    discriminants: &crate::Discriminants,
    body: &mut Body,
) {
    let layouts = crate::layout::Layouts::new(items, sigs, discriminants);
    let blocks: Vec<_> = body.blocks.iter().map(|(b, _)| b).collect();
    for block in blocks {
        let range = body.blocks[block].terminator.range;
        let TerminatorKind::Call {
            func,
            generic_args,
            args,
            dest,
            target,
        } = body.blocks[block].terminator.kind.clone()
        else {
            continue;
        };
        let data = &items.functions[func];
        let Some(intrinsic) = data.intrinsic else {
            continue;
        };
        if intrinsic.is_coroutine() {
            continue;
        }
        if intrinsic == flux_hir::Intrinsic::BoxCallOnce {
            if generic_args[1].is_dyn() {
                // The intrinsic retains its Box receiver type in MIR. Codegen
                // calls the object's owning slot with the thin box pointer.
                continue;
            }
            let instance = box_call_once(items, generic_args);
            body.blocks[block].terminator.kind = TerminatorKind::Call {
                func: instance.function,
                generic_args: instance.args,
                args,
                dest,
                target,
            };
            continue;
        }
        if intrinsic == flux_hir::Intrinsic::SizedDispatch {
            // Collection rejects unresolved selectors. Keeping an unexpanded
            // inspection body intact avoids inventing an arbitrary branch.
            if let Ok(function) = flux_ty::sized_dispatch::select(sigs, func, generic_args) {
                body.blocks[block].terminator.kind = TerminatorKind::Call {
                    func: function,
                    generic_args,
                    args,
                    dest,
                    target,
                };
            }
            continue;
        }
        let goto = match target {
            Some(target) => TerminatorKind::Goto(target),
            None => TerminatorKind::Unreachable,
        };
        if matches!(
            intrinsic.name(),
            "caller_location"
                | "read_unaligned"
                | "read_volatile"
                | "write_volatile"
                | "transmute"
                | "fmaf32"
                | "fmaf64"
        ) || intrinsic.is_neon()
            || intrinsic.is_x86()
            || intrinsic.is_bit_count()
            || atomic_intrinsic(intrinsic.name()).is_some()
        {
            // Code generation's: where the call is.
            continue;
        }
        if intrinsic == flux_hir::Intrinsic::PtrWithAddr {
            body.blocks[block].statements.push(Statement {
                kind: StatementKind::Assign(
                    dest,
                    Rvalue::WithAddr(args[0].clone(), args[1].clone()),
                ),
                range,
            });
            body.blocks[block].terminator.kind = goto;
            continue;
        }
        if intrinsic.name() == "unreachable_unchecked" {
            body.blocks[block].terminator.kind = TerminatorKind::Unreachable;
            continue;
        }
        if intrinsic.name() == "uninit" {
            // Nothing: its memory is written before it is read.
            body.blocks[block].terminator.kind = goto;
            continue;
        }
        // Of a trait object: through its vtable, code generation's.
        let arg_ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
        let name = intrinsic.name();
        if (matches!(name, "size_of_val" | "align_of_val")
            && sigs.metadata(arg_ty) == flux_ty::Metadata::VTable)
            || (name == "drop_in_place" && arg_ty.is_dyn())
        {
            continue;
        }
        if name == "drop_in_place" {
            expand_drop_in_place(items, sigs, body, block, &args[0], arg_ty, (target, range));
            continue;
        }
        // A reference argument, in a temporary to dereference.
        let pointer = |body: &mut Body, op: &Operand| -> Place {
            let ty = body.operand_ty(op, sigs);
            let temp = body.locals.alloc(LocalDecl {
                ty,
                kind: LocalKind::Temp,
            });
            body.blocks[block].statements.push(Statement {
                kind: StatementKind::Assign(temp.into(), Rvalue::Use(op.clone())),
                range,
            });
            Place::from(temp)
        };
        let (place, rvalue) = match (intrinsic.name(), args.as_slice()) {
            ("discriminant_is_signed", []) => (
                dest,
                Rvalue::Use(Operand::Const(Constant::Bool(
                    sigs.discriminant_ty(flux_solve::traits::normalize(items, sigs, &[], arg_ty))
                        .is_some_and(flux_ty::IntTy::is_signed),
                ))),
            ),
            ("discriminant_value", [value]) => {
                let Some(ty) =
                    sigs.discriminant_ty(flux_solve::traits::normalize(items, sigs, &[], arg_ty))
                else {
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(
                            dest,
                            Rvalue::Use(Operand::Const(Constant::Int {
                                bits: 0,
                                ty: flux_ty::IntTy::U128,
                            })),
                        ),
                        range,
                    });
                    body.blocks[block].terminator.kind = goto;
                    continue;
                };
                let value = pointer(body, value);
                let tag = body.locals.alloc(LocalDecl {
                    ty: Ty::int(ty),
                    kind: LocalKind::Temp,
                });
                body.blocks[block].statements.push(Statement {
                    kind: StatementKind::Assign(tag.into(), Rvalue::Discriminant(value.deref())),
                    range,
                });
                (
                    dest,
                    Rvalue::Cast(Operand::Copy(tag.into()), Ty::int(flux_ty::IntTy::U128)),
                )
            }
            ("is_copy", []) => {
                let ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
                let copy = flux_solve::traits::is_copy(items, sigs, &[], ty);
                (dest, Rvalue::Use(Operand::Const(Constant::Bool(copy))))
            }
            ("needs_drop", []) => {
                let ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
                let needs = flux_solve::traits::needs_drop(items, sigs, &[], ty);
                (dest, Rvalue::Use(Operand::Const(Constant::Bool(needs))))
            }
            (name @ ("size_of_val" | "align_of_val"), [ptr]) if sigs.is_unsized(arg_ty) => {
                let (prefix, stride, align) = layouts
                    .length_layout(arg_ty)
                    .expect("represented DST length layout");
                let usize = flux_ty::IntTy::Usize;
                let constant = |bits| {
                    Operand::Const(Constant::Int {
                        bits: bits as u128,
                        ty: usize,
                    })
                };
                if name == "align_of_val" {
                    (dest, Rvalue::Use(constant(align)))
                } else {
                    let ptr = pointer(body, ptr);
                    let len = body.locals.alloc(LocalDecl {
                        ty: Ty::int(usize),
                        kind: LocalKind::Temp,
                    });
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(
                            len.into(),
                            Rvalue::Len {
                                place: ptr.deref(),
                                borrows: false,
                            },
                        ),
                        range,
                    });
                    let size = body.locals.alloc(LocalDecl {
                        ty: Ty::int(usize),
                        kind: LocalKind::Temp,
                    });
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(
                            size.into(),
                            Rvalue::BinaryOp(
                                crate::BinOp::Mul,
                                Operand::Copy(len.into()),
                                constant(stride),
                            ),
                        ),
                        range,
                    });
                    let padded = body.locals.alloc(LocalDecl {
                        ty: Ty::int(usize),
                        kind: LocalKind::Temp,
                    });
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(
                            padded.into(),
                            Rvalue::BinaryOp(
                                crate::BinOp::Add,
                                Operand::Copy(size.into()),
                                constant(prefix + align - 1),
                            ),
                        ),
                        range,
                    });
                    (
                        dest,
                        Rvalue::BinaryOp(
                            crate::BinOp::BitAnd,
                            Operand::Copy(padded.into()),
                            constant(!(align - 1)),
                        ),
                    )
                }
            }
            (name @ ("size_of_val" | "align_of_val"), [_]) => {
                let layout = layouts.of(arg_ty);
                let value = if name == "size_of_val" {
                    layout.size
                } else {
                    layout.align
                };
                let constant = Constant::Int {
                    bits: u128::from(value),
                    ty: flux_ty::IntTy::Usize,
                };
                (dest, Rvalue::Use(Operand::Const(constant)))
            }
            ("type_id", []) => {
                let bits = flux_solve::type_identity::of(items, sigs, arg_ty)
                    .expect("monomorphized type identity");
                (
                    dest,
                    Rvalue::Use(Operand::Const(Constant::Int {
                        bits,
                        ty: flux_ty::IntTy::U128,
                    })),
                )
            }
            ("type_name", []) => {
                // A static string, independent of values or borrows of T.
                // The public API promises a diagnostic description, not a
                // unique name or rustc's exact formatting.
                let name = arg_ty.to_string();
                let bytes = name
                    .bytes()
                    .map(|b| crate::eval::Value::Int {
                        bits: u128::from(b),
                        ty: flux_ty::IntTy::U8,
                    })
                    .collect();
                let index = body.promoted.len() as u32;
                body.promoted.push(crate::eval::Value::Aggregate(bytes));
                (
                    dest,
                    Rvalue::Use(Operand::Const(Constant::Promoted {
                        index,
                        ty: Ty::new(TyKind::Ref(Mutability::Not, Ty::STR)),
                    })),
                )
            }
            (name @ ("size_of" | "align_of"), []) => {
                let ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
                let layout = layouts.of(ty);
                let value = if name == "size_of" {
                    layout.size
                } else {
                    layout.align
                };
                let constant = Constant::Int {
                    bits: u128::from(value),
                    ty: flux_ty::IntTy::Usize,
                };
                (dest, Rvalue::Use(Operand::Const(constant)))
            }
            (name @ ("ptr_add" | "ptr_sub" | "ptr_offset"), [ptr, count]) => {
                let usize_ty = Ty::int(flux_ty::IntTy::Usize);
                let mut temp = |ty, value| {
                    let local = body.locals.alloc(LocalDecl {
                        ty,
                        kind: LocalKind::Temp,
                    });
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(local.into(), value),
                        range,
                    });
                    Operand::Move(local.into())
                };
                let address = temp(usize_ty, Rvalue::Cast(ptr.clone(), usize_ty));
                let count = temp(usize_ty, Rvalue::Cast(count.clone(), usize_ty));
                let size = Operand::Const(Constant::Int {
                    bits: u128::from(layouts.of(arg_ty).size),
                    ty: flux_ty::IntTy::Usize,
                });
                let bytes = temp(usize_ty, Rvalue::BinaryOp(crate::BinOp::Mul, count, size));
                let op = if name == "ptr_sub" {
                    crate::BinOp::Sub
                } else {
                    crate::BinOp::Add
                };
                let address = temp(usize_ty, Rvalue::BinaryOp(op, address, bytes));
                let ty = body.place_ty(&dest, sigs);
                (dest, Rvalue::Cast(address, ty))
            }
            ("read", [src]) => {
                let src = pointer(body, src);
                (dest, Rvalue::Use(Operand::Copy(src.deref())))
            }
            ("raw_slice_len", [ptr]) => {
                let ptr = pointer(body, ptr);
                (
                    dest,
                    Rvalue::Len {
                        place: ptr.deref(),
                        borrows: false,
                    },
                )
            }
            ("write", [dst, value]) => {
                let dst = pointer(body, dst);
                (dst.deref(), Rvalue::Use(value.clone()))
            }
            ("subslice" | "subslice_mut", [slice, start, end]) => (
                dest,
                Rvalue::Subslice {
                    slice: slice.clone(),
                    start: start.clone(),
                    end: end.clone(),
                },
            ),
            // `data` as a pointer to an empty array, unsized, as a
            // reference, then its first `len` elements.
            (
                "slice_from_raw_parts"
                | "slice_from_raw_parts_mut"
                | "raw_slice_from_parts"
                | "raw_slice_from_parts_mut",
                [data, len],
            ) => {
                let slice_ref = body.place_ty(&dest, sigs);
                let (m, elem) = match slice_ref.kind() {
                    TyKind::Ref(m, slice) => (m, slice.sequence_elem().unwrap_or(Ty::ERROR)),
                    TyKind::RawPtr(m, slice) => (m, slice.sequence_elem().unwrap_or(Ty::ERROR)),
                    _ => (Mutability::Not, Ty::ERROR),
                };
                let temp = |body: &mut Body, ty: Ty, rvalue: Rvalue| -> Operand {
                    let local = body.locals.alloc(LocalDecl {
                        ty,
                        kind: LocalKind::Temp,
                    });
                    body.blocks[block].statements.push(Statement {
                        kind: StatementKind::Assign(local.into(), rvalue),
                        range,
                    });
                    Operand::Move(local.into())
                };
                let array = Ty::new(TyKind::RawPtr(m, Ty::array(elem, 0)));
                let raw_slice = Ty::new(TyKind::RawPtr(m, Ty::new(TyKind::Slice(elem))));
                let array_ptr = temp(body, array, Rvalue::Cast(data.clone(), array));
                let raw = temp(body, raw_slice, Rvalue::Unsize(array_ptr, raw_slice));
                let empty = temp(body, slice_ref, Rvalue::Cast(raw, slice_ref));
                let zero = Operand::Const(Constant::Int {
                    bits: 0,
                    ty: flux_ty::IntTy::Usize,
                });
                (
                    dest,
                    Rvalue::Subslice {
                        slice: empty,
                        start: zero,
                        end: len.clone(),
                    },
                )
            }
            _ => unreachable!("`core`'s intrinsics are these"),
        };
        let data = &mut body.blocks[block];
        data.statements.push(Statement {
            kind: StatementKind::Assign(place, rvalue),
            range,
        });
        data.terminator.kind = goto;
    }
}

/// `drop_in_place::<ty>(ptr)`, which continues at `target`: a drop of
/// `*ptr`, or for a slice, of each element in turn, in a loop.
fn expand_drop_in_place(
    items: &ItemTree,
    sigs: &Signatures,
    body: &mut Body,
    block: BasicBlock,
    ptr: &Operand,
    ty: Ty,
    (target, range): (Option<BasicBlock>, flux_syntax::TextRange),
) {
    let Some(target) = target else {
        body.blocks[block].terminator.kind = TerminatorKind::Unreachable;
        return;
    };
    // Types have been substituted: only traverse elements whose drop may run
    // code. Size alone is insufficient, since even a ZST can have a destructor.
    // This elides only the pointee's drops, not its owner's destructor/free.
    if let TyKind::Slice(elem) = ty.kind()
        && !flux_solve::traits::needs_drop(items, sigs, &[], elem)
    {
        body.blocks[block].terminator.kind = TerminatorKind::Goto(target);
        return;
    }
    let temp = |body: &mut Body, ty: Ty| {
        body.locals.alloc(LocalDecl {
            ty,
            kind: LocalKind::Temp,
        })
    };
    let assign = |place: Place, rvalue: Rvalue| Statement {
        kind: StatementKind::Assign(place, rvalue),
        range,
    };
    let ptr_ty = body.operand_ty(ptr, sigs);
    let pointer = temp(body, ptr_ty);
    body.blocks[block]
        .statements
        .push(assign(pointer.into(), Rvalue::Use(ptr.clone())));
    let place = Place::from(pointer).deref();
    let TyKind::Slice(_) = ty.kind() else {
        if ty == Ty::STR {
            body.blocks[block].terminator.kind = TerminatorKind::Goto(target);
        } else {
            body.blocks[block].terminator.kind = TerminatorKind::Drop { place, target };
        }
        return;
    };
    // for i in 0..len { drop((*p)[i]) }
    let usize = Ty::int(flux_ty::IntTy::Usize);
    let (i, len, more) = (temp(body, usize), temp(body, usize), temp(body, Ty::BOOL));
    let zero = Operand::Const(Constant::int(0, flux_ty::IntTy::Usize));
    let one = Operand::Const(Constant::int(1, flux_ty::IntTy::Usize));
    body.blocks[block]
        .statements
        .push(assign(i.into(), Rvalue::Use(zero)));
    body.blocks[block].statements.push(assign(
        len.into(),
        Rvalue::Len {
            place: place.clone(),
            borrows: false,
        },
    ));
    let head = body.blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Unreachable,
            range,
        },
    });
    let step = body.blocks.alloc(BasicBlockData {
        statements: vec![assign(
            i.into(),
            Rvalue::BinaryOp(crate::BinOp::Add, Operand::Copy(i.into()), one),
        )],
        terminator: Terminator {
            kind: TerminatorKind::Goto(head),
            range,
        },
    });
    let drop = body.blocks.alloc(BasicBlockData {
        statements: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Drop {
                place: place.index(i),
                target: step,
            },
            range,
        },
    });
    body.blocks[head].statements.push(assign(
        more.into(),
        Rvalue::BinaryOp(
            crate::BinOp::Lt,
            Operand::Copy(i.into()),
            Operand::Copy(len.into()),
        ),
    ));
    body.blocks[head].terminator.kind = TerminatorKind::SwitchInt {
        discr: Operand::Copy(more.into()),
        targets: vec![(0, target)],
        otherwise: drop,
        variants: None,
    };
    body.blocks[block].terminator.kind = TerminatorKind::Goto(head);
}

/// `Drop::drop`.
fn drop_fn(items: &ItemTree) -> Option<FunctionId> {
    let drop = flux_intern::sym::drop;
    let t = *items.lang_traits.get(&drop)?;
    items.traits[t].function(items, drop)
}

/// What dropping a value runs: its `Drop` impls, the parts of it whose
/// types have one, in the order dropping it runs them.
enum Glue {
    /// `Drop::drop(&mut place)`, for a `place` of type `ty`.
    Drop(Place, Ty),
    /// Move the whole unaligned field before invoking its destructor.
    Unaligned(Place, Ty, Vec<Glue>),
    /// For an enum at `place`, the glue of the variant it holds.
    Variants(Place, StructId, Vec<Vec<Glue>>),
    /// For an array of `len` elements, the glue of each, in order, at
    /// places indexed by `index`, a stand-in for the loop's counter.
    Elements {
        index: Local,
        len: GlueLen,
        glue: Vec<Glue>,
    },
}

enum GlueLen {
    Constant(u64),
    Slice(Place),
}

/// The length of an array in a monomorphized body, `len`, which is known.
pub(crate) fn mono_len(len: Ty) -> u64 {
    len.as_usize()
        .expect("arrays' lengths are known once monomorphized")
}

/// The stand-in for the counter of the `depth`th loop over an array's
/// elements: a local no body has.
fn element_index(depth: usize) -> Local {
    Local::from_raw(u32::MAX - depth as u32)
}

/// The types whose `Drop` impls `glue` runs.
fn glue_types(glue: &[Glue], out: &mut Vec<Ty>) {
    for part in glue {
        match part {
            Glue::Drop(_, ty) => out.push(*ty),
            Glue::Variants(_, _, variants) => {
                for variant in variants {
                    glue_types(variant, out);
                }
            }
            Glue::Elements { glue, .. } | Glue::Unaligned(_, _, glue) => glue_types(glue, out),
        }
    }
}

/// The glue of a `ty` at `place`: a struct's own `Drop` impl, then its
/// fields', in order; an enum's, then the fields' of the variant it
/// holds.
fn drop_glue(items: &ItemTree, sigs: &Signatures, place: Place, ty: Ty, out: &mut Vec<Glue>) {
    match ty.kind() {
        TyKind::Adt(adt, _) if flux_solve::is_lang_struct(items, adt.strukt, "manually_drop") => {
            return;
        }
        TyKind::Adt(adt, _) => {
            if flux_solve::traits::drop_impl(items, sigs, adt.strukt).is_some() {
                out.push(Glue::Drop(place.clone(), ty));
            }
            if let Some(variants) = flux_solve::variants(sigs, ty) {
                let glue: Vec<Vec<Glue>> = variants
                    .iter()
                    .enumerate()
                    .map(|(v, fields)| {
                        let mut glue = Vec::new();
                        let variant = place.downcast(v as u32);
                        for (i, &field) in fields.iter().enumerate() {
                            drop_glue(items, sigs, variant.field(i as u32), field, &mut glue);
                        }
                        glue
                    })
                    .collect();
                if glue.iter().any(|g| !g.is_empty()) {
                    out.push(Glue::Variants(place, adt.strukt, glue));
                }
                return;
            }
        }
        TyKind::Tuple(_) | TyKind::Closure(..) => {}
        TyKind::Array(elem, len) => {
            let len = mono_len(len);
            let depth = place.index_locals().count();
            let index = element_index(depth);
            let mut glue = Vec::new();
            drop_glue(items, sigs, place.index(index), elem, &mut glue);
            if !glue.is_empty() && len > 0 {
                out.push(Glue::Elements {
                    index,
                    len: GlueLen::Constant(len),
                    glue,
                });
            }
            return;
        }
        TyKind::Slice(elem) => {
            let index = element_index(place.index_locals().count());
            let mut glue = Vec::new();
            drop_glue(items, sigs, place.index(index), elem, &mut glue);
            if !glue.is_empty() {
                out.push(Glue::Elements {
                    index,
                    len: GlueLen::Slice(place),
                    glue,
                });
            }
            return;
        }
        _ => return,
    }
    for (i, &field) in flux_solve::fields(sigs, ty)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let field_place = place.field(i as u32);
        let packed =
            matches!(ty.kind(),TyKind::Adt(adt,_) if items.structs[adt.strukt].repr_pack.is_some());
        if packed {
            let mut glue = Vec::new();
            drop_glue(items, sigs, field_place.clone(), field, &mut glue);
            if !glue.is_empty() {
                out.push(Glue::Unaligned(field_place, field, glue));
            }
        } else {
            drop_glue(items, sigs, field_place, field, out);
        }
    }
}

/// Replaces each drop in `body`, whose types are known, with its drop
/// glue: `Drop::drop(&mut place)` for each part that has a `Drop` impl,
/// and for enums, a test of the variant they hold.
pub fn expand_drops(items: &ItemTree, sigs: &Signatures, body: &mut Body) {
    let Some(drop) = drop_fn(items) else {
        return;
    };
    let unit = body.locals.alloc(LocalDecl {
        ty: Ty::UNIT,
        kind: LocalKind::Temp,
    });
    let blocks: Vec<_> = body.blocks.iter().map(|(b, _)| b).collect();
    for block in blocks {
        let range = body.blocks[block].terminator.range;
        let TerminatorKind::Drop { place, target } = body.blocks[block].terminator.kind.clone()
        else {
            continue;
        };
        let ty = body.place_ty(&place, sigs);
        let mut glue = Vec::new();
        drop_glue(items, sigs, place, ty, &mut glue);
        let mut expand = Expand {
            items,
            sigs,
            body,
            drop,
            unit,
            range,
        };
        let (statements, kind) = expand.glue(&glue, target);
        let data = &mut body.blocks[block];
        data.statements.extend(statements);
        data.terminator.kind = kind;
    }
}

/// Builds the blocks that run drop glue.
struct Expand<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    body: &'a mut Body,
    drop: FunctionId,
    unit: Local,
    range: flux_syntax::TextRange,
}

impl Expand<'_> {
    /// The statements and terminator that run `glue`, then go to
    /// `target`.
    fn glue(&mut self, glue: &[Glue], target: BasicBlock) -> (Vec<Statement>, TerminatorKind) {
        let mut next = (Vec::new(), TerminatorKind::Goto(target));
        for (i, part) in glue.iter().enumerate().rev() {
            let after = if i + 1 == glue.len() {
                target
            } else {
                self.push(next)
            };
            next = self.part(part, after);
        }
        next
    }

    fn push(&mut self, (statements, kind): (Vec<Statement>, TerminatorKind)) -> BasicBlock {
        self.body.blocks.alloc(BasicBlockData {
            statements,
            terminator: Terminator {
                kind,
                range: self.range,
            },
        })
    }

    fn part(&mut self, part: &Glue, target: BasicBlock) -> (Vec<Statement>, TerminatorKind) {
        match part {
            Glue::Unaligned(place, ty, _) => {
                let local = self.body.locals.alloc(LocalDecl {
                    ty: *ty,
                    kind: LocalKind::Temp,
                });
                let mut glue = Vec::new();
                drop_glue(self.items, self.sigs, local.into(), *ty, &mut glue);
                let (mut statements, kind) = self.glue(&glue, target);
                statements.insert(
                    0,
                    Statement {
                        kind: StatementKind::Assign(
                            local.into(),
                            Rvalue::Use(Operand::Move(place.clone())),
                        ),
                        range: self.range,
                    },
                );
                (statements, kind)
            }
            Glue::Drop(place, ty) => {
                let pointer = self.body.locals.alloc(LocalDecl {
                    ty: Ty::new(TyKind::Ref(Mutability::Mut, *ty)),
                    kind: LocalKind::Temp,
                });
                let borrow = Statement {
                    kind: StatementKind::Assign(
                        pointer.into(),
                        Rvalue::Ref {
                            raw: false,
                            mutability: Mutability::Mut,
                            place: place.clone(),
                            two_phase: false,
                        },
                    ),
                    range: self.range,
                };
                let call = TerminatorKind::Call {
                    func: self.drop,
                    generic_args: Ty::list(&[*ty]),
                    args: vec![Operand::Move(pointer.into())],
                    dest: self.unit.into(),
                    target: Some(target),
                };
                (vec![borrow], call)
            }
            Glue::Variants(place, strukt, variants) => {
                let mut targets = Vec::new();
                for (v, glue) in variants.iter().enumerate() {
                    if glue.is_empty() {
                        continue;
                    }
                    let run = self.glue(glue, target);
                    let block = self.push(run);
                    targets.push((v as u128, block));
                }
                let discriminant = self.body.locals.alloc(LocalDecl {
                    ty: Ty::int(flux_ty::discriminant_ty(self.items, *strukt)),
                    kind: LocalKind::Temp,
                });
                let read = Statement {
                    kind: StatementKind::Assign(
                        discriminant.into(),
                        Rvalue::Discriminant(place.clone()),
                    ),
                    range: self.range,
                };
                let switch = TerminatorKind::SwitchInt {
                    discr: Operand::Copy(discriminant.into()),
                    targets,
                    otherwise: target,
                    variants: Some(*strukt),
                };
                (vec![read], switch)
            }
            // `i = 0; while i < len { glue(place[i]); i += 1 }`
            Glue::Elements { index, len, glue } => {
                let usize = Ty::int(flux_ty::IntTy::Usize);
                let counter = self.body.locals.alloc(LocalDecl {
                    ty: usize,
                    kind: LocalKind::Temp,
                });
                let within = self.body.locals.alloc(LocalDecl {
                    ty: Ty::BOOL,
                    kind: LocalKind::Temp,
                });
                let glue: Vec<Glue> = glue.iter().map(|g| g.reindex(*index, counter)).collect();
                let assign = |place: Local, rvalue: Rvalue, range| Statement {
                    kind: StatementKind::Assign(place.into(), rvalue),
                    range,
                };
                let int = |value: u64| {
                    Operand::Const(Constant::int(i128::from(value), flux_ty::IntTy::Usize))
                };
                let mut initial = vec![assign(counter, Rvalue::Use(int(0)), self.range)];
                let bound = match len {
                    GlueLen::Constant(len) => int(*len),
                    GlueLen::Slice(place) => {
                        let length = self.body.locals.alloc(LocalDecl {
                            ty: usize,
                            kind: LocalKind::Temp,
                        });
                        initial.push(assign(
                            length,
                            Rvalue::Len {
                                place: place.clone(),
                                borrows: false,
                            },
                            self.range,
                        ));
                        Operand::Copy(length.into())
                    }
                };
                let head = self.push((Vec::new(), TerminatorKind::Unreachable));
                let step = self.push((
                    vec![assign(
                        counter,
                        Rvalue::BinaryOp(crate::BinOp::Add, Operand::Copy(counter.into()), int(1)),
                        self.range,
                    )],
                    TerminatorKind::Goto(head),
                ));
                let run = self.glue(&glue, step);
                let run = self.push(run);
                self.body.blocks[head] = BasicBlockData {
                    statements: vec![assign(
                        within,
                        Rvalue::BinaryOp(crate::BinOp::Lt, Operand::Copy(counter.into()), bound),
                        self.range,
                    )],
                    terminator: Terminator {
                        kind: TerminatorKind::SwitchInt {
                            discr: Operand::Copy(within.into()),
                            targets: vec![(0, target)],
                            otherwise: run,
                            variants: None,
                        },
                        range: self.range,
                    },
                };
                (initial, TerminatorKind::Goto(head))
            }
        }
    }
}

impl Glue {
    /// The glue with the places indexed by `from` indexed by `to`.
    fn reindex(&self, from: Local, to: Local) -> Glue {
        let place = |p: &Place| Place {
            local: p.local,
            projection: crate::Projection::from_exact_iter(p.projection.iter().map(|&e| match e {
                crate::ProjectionElem::Index(l) if l == from => crate::ProjectionElem::Index(to),
                e => e,
            })),
        };
        match self {
            Glue::Drop(p, ty) => Glue::Drop(place(p), *ty),
            Glue::Unaligned(p, ty, glue) => Glue::Unaligned(
                place(p),
                *ty,
                glue.iter().map(|g| g.reindex(from, to)).collect(),
            ),
            Glue::Variants(p, strukt, variants) => Glue::Variants(
                place(p),
                *strukt,
                variants
                    .iter()
                    .map(|v| v.iter().map(|g| g.reindex(from, to)).collect())
                    .collect(),
            ),
            Glue::Elements { index, len, glue } => Glue::Elements {
                index: *index,
                len: match len {
                    GlueLen::Constant(len) => GlueLen::Constant(*len),
                    GlueLen::Slice(p) => GlueLen::Slice(place(p)),
                },
                glue: glue.iter().map(|g| g.reindex(from, to)).collect(),
            },
        }
    }
}

/// `f::<A, B>` as rustc writes an instance in messages, with types
/// nested more than nine deep written `_`.
fn instance_name(hir: &Hir, instance: Instance) -> String {
    let name = hir
        .items
        .function_path(&hir.items.functions[instance.function]);
    let args: Vec<String> = instance.args.iter().map(|&a| truncated(a, 9)).collect();
    format!("{name}::<{}>", args.join(", "))
}

fn truncated(ty: Ty, depth: u32) -> String {
    if depth == 0 {
        return "_".to_owned();
    }
    match ty.kind() {
        TyKind::Tuple(elements) if !elements.is_empty() => {
            let elements: Vec<String> = elements.iter().map(|&e| truncated(e, depth - 1)).collect();
            if elements.len() == 1 {
                format!("({},)", elements[0])
            } else {
                format!("({})", elements.join(", "))
            }
        }
        TyKind::Adt(adt, args) if !args.is_empty() => {
            let args: Vec<String> = args.iter().map(|&a| truncated(a, depth - 1)).collect();
            format!("{}<{}>", adt.name, args.join(", "))
        }
        _ => ty.to_string(),
    }
}

/// `body`, of a generic function, with generic arguments `args`
/// substituted for its parameters.
pub fn subst_body(hir: &Hir, sigs: &Signatures, body: &Body, args: &[Ty]) -> Body {
    map_body_types(body, args, |ty| {
        flux_solve::traits::instance_ty(&hir.items, sigs, ty, args)
    })
}

/// Substitutes a library template while retaining its unresolved projections.
/// Symbolic reachability has no concrete instance with which to select impls.
/// Concrete monomorphization must continue to use [`subst_body`].
pub fn subst_template(body: &Body, args: &[Ty]) -> Body {
    map_body_types(body, args, |ty| ty.subst(args))
}

fn map_body_types(body: &Body, args: &[Ty], subst: impl Fn(Ty) -> Ty) -> Body {
    let mut body = body.clone();
    if !args.is_empty() {
        for value in &mut body.promoted {
            *value = value.map_functions(&|instance| Instance {
                function: instance.function,
                args: Ty::list(
                    &instance
                        .args
                        .iter()
                        .map(|&ty| subst(ty))
                        .collect::<Vec<_>>(),
                ),
            });
        }
    }
    for local in body.locals.values_mut() {
        local.ty = subst(local.ty);
    }
    let operand = |op: &mut Operand| {
        if let Operand::Const(Constant::FnItem(ty) | Constant::Promoted { ty, .. }) = op {
            *ty = subst(*ty);
        }
        if let Operand::Const(Constant::TyConst { value, ty }) = *op {
            *op = Operand::Const(Constant::of_ty_const(subst(value), subst(ty)));
        }
        if let Operand::Const(
            Constant::Item {
                ty,
                args: item_args,
                ..
            }
            | Constant::ItemRef {
                ty,
                args: item_args,
                ..
            }
            | Constant::Fn {
                ty,
                args: item_args,
                ..
            },
        ) = op
        {
            *ty = subst(*ty);
            *item_args = Ty::list(&item_args.iter().map(|&a| subst(a)).collect::<Vec<_>>());
        }
    };
    for data in body.blocks.values_mut() {
        for statement in &mut data.statements {
            let StatementKind::Assign(_, rvalue) = &mut statement.kind else {
                continue;
            };
            match rvalue {
                Rvalue::OffsetOf(ty, _) => *ty = subst(*ty),
                Rvalue::Use(op) | Rvalue::UnaryOp(_, op) => operand(op),
                Rvalue::Cast(op, ty) => {
                    operand(op);
                    *ty = subst(*ty);
                }
                Rvalue::WithAddr(l, r) | Rvalue::BinaryOp(_, l, r) => {
                    operand(l);
                    operand(r);
                }
                Rvalue::InlineAsm { inputs, .. } => inputs.iter_mut().for_each(operand),
                Rvalue::Aggregate(kind, ops) => {
                    if let crate::AggregateKind::Array(elem) = kind {
                        *elem = subst(*elem);
                    }
                    ops.iter_mut().for_each(operand)
                }
                Rvalue::Repeat(op, count) => {
                    operand(op);
                    *count = subst(*count);
                }
                Rvalue::Unsize(op, ty) => {
                    operand(op);
                    *ty = subst(*ty);
                }
                Rvalue::Subslice { slice, start, end } => {
                    operand(slice);
                    operand(start);
                    operand(end);
                }
                Rvalue::Ref { .. }
                | Rvalue::Discriminant(_)
                | Rvalue::DiscriminantForDrop(_)
                | Rvalue::Len { .. } => {}
            }
        }
        match &mut data.terminator.kind {
            TerminatorKind::Call {
                generic_args,
                args: call_args,
                ..
            } => {
                let list: Vec<Ty> = generic_args.iter().map(|&a| subst(a)).collect();
                *generic_args = Ty::list(&list);
                call_args.iter_mut().for_each(operand);
            }
            TerminatorKind::CallIndirect {
                callee,
                args: call_args,
                ..
            } => {
                operand(callee);
                call_args.iter_mut().for_each(operand);
            }
            TerminatorKind::SwitchInt { discr, .. } => operand(discr),
            TerminatorKind::Assert { cond, msg, .. } => {
                operand(cond);
                match msg {
                    AssertMessage::DivisionByZero(a) | AssertMessage::RemainderByZero(a) => {
                        operand(a)
                    }
                    AssertMessage::DivisionOverflow(a, b)
                    | AssertMessage::RemainderOverflow(a, b)
                    | AssertMessage::BoundsCheck { len: a, index: b } => {
                        operand(a);
                        operand(b);
                    }
                }
            }
            TerminatorKind::Goto(_)
            | TerminatorKind::Drop { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => {}
        }
    }
    body
}

/// The one machine scalar represented by a bounded volatile operation.
/// Enum, aggregate and fat-pointer accesses are not silently decomposed.
pub fn volatile_scalar(sigs: &Signatures, mut ty: Ty) -> Option<Ty> {
    for _ in 0..128 {
        match ty.kind() {
            TyKind::Int(int) if int.bits() <= 64 => return Some(ty),
            TyKind::Bool | TyKind::Char | TyKind::Float(_) | TyKind::FnPtr(_) => return Some(ty),
            TyKind::RawPtr(_, pointee) | TyKind::Ref(_, pointee) if !sigs.is_unsized(pointee) => {
                return Some(ty);
            }
            TyKind::Tuple([]) => return Some(Ty::UNIT),
            TyKind::Adt(..) if flux_solve::variants(sigs, ty).is_none() => {
                let fields = flux_solve::fields(sigs, ty)?;
                match fields {
                    [] => return Some(Ty::UNIT),
                    [field] => ty = *field,
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    None
}

/// Scalar storage identity for the private native atomic bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicWord {
    Usize,
    U8,
    U16,
    U32,
    Ptr,
}

/// Exact bridge names and fixed valid orderings; no runtime ordering crosses this boundary.
pub fn atomic_intrinsic(name: &str) -> Option<(&str, &str, AtomicWord)> {
    let (rest, word) = if let Some(rest) = name.strip_prefix("__atomic_usize_") {
        (rest, AtomicWord::Usize)
    } else if let Some(rest) = name.strip_prefix("__atomic_u8_") {
        (rest, AtomicWord::U8)
    } else if let Some(rest) = name.strip_prefix("__atomic_u16_") {
        (rest, AtomicWord::U16)
    } else if let Some(rest) = name.strip_prefix("__atomic_u32_") {
        (rest, AtomicWord::U32)
    } else {
        (name.strip_prefix("__atomic_ptr_")?, AtomicWord::Ptr)
    };
    let (op, order) = rest.split_once('_')?;
    let valid = match op {
        "load" => matches!(order, "relaxed" | "acquire" | "seqcst"),
        "store" => matches!(order, "relaxed" | "release" | "seqcst"),
        "add" | "sub" | "cas" => matches!(
            order,
            "relaxed" | "acquire" | "release" | "acqrel" | "seqcst"
        ),
        "fence" => matches!(order, "acquire" | "release" | "acqrel" | "seqcst"),
        _ => false,
    };
    let valid = valid
        && match word {
            AtomicWord::Usize => true,
            AtomicWord::U8 => matches!(op, "load" | "store" | "cas"),
            AtomicWord::U16 => matches!(op, "load" | "store" | "cas"),
            AtomicWord::U32 => matches!(op, "load" | "store" | "add" | "cas"),
            AtomicWord::Ptr => matches!(op, "load" | "store" | "cas"),
        };
    valid.then_some((op, order, word))
}

fn valid_atomic_signature(op: &str, word: AtomicWord, args: &[Ty], sig: &flux_ty::FnSig) -> bool {
    if op == "fence" {
        return word == AtomicWord::Usize
            && args.is_empty()
            && sig.params.is_empty()
            && sig.ret == Ty::UNIT;
    }
    let word = match word {
        AtomicWord::Usize => Ty::int(flux_ty::IntTy::Usize),
        AtomicWord::U8 if matches!(op, "load" | "store" | "cas") => Ty::int(flux_ty::IntTy::U8),
        AtomicWord::U16 if matches!(op, "load" | "store" | "cas") => Ty::int(flux_ty::IntTy::U16),
        AtomicWord::U32 if matches!(op, "load" | "store" | "add" | "cas") => {
            Ty::int(flux_ty::IntTy::U32)
        }
        AtomicWord::Ptr if matches!(op, "load" | "store" | "cas") => {
            let Some(&ty) = args.first() else {
                return false;
            };
            if !matches!(ty.kind(), TyKind::RawPtr(Mutability::Mut, pointee) if !pointee.is_unsized() && !pointee.has_params())
            {
                return false;
            }
            ty
        }
        _ => return false,
    };
    let ptr = Ty::raw_ptr(
        if op == "load" {
            Mutability::Not
        } else {
            Mutability::Mut
        },
        word,
    );
    let params: &[Ty] = match op {
        "load" => &[ptr],
        "store" | "add" | "sub" => &[ptr, word],
        "cas" => &[ptr, word, word],
        _ => return false,
    };
    args == [word]
        && sig.params.as_ref() == params
        && sig.ret == if op == "store" { Ty::UNIT } else { word }
}

#[cfg(test)]
mod const_pointer_tests {
    #[test]
    fn recursive_fields_can_introduce_function_addresses_after_substitution() {
        let (_, _, hir) = flux_hir::lower_text(
            "pointer_storage.rs",
            "
            trait Data { type Out; }
            struct Start;
            struct Next<T>(T);
            impl Data for Start { type Out = u32; }
            impl<T> Data for Next<T> { type Out = fn() -> u32; }
            struct Link<T:Data> { data:T::Out, next:*const Link<Next<T>> }
            fn pointer_storage_probe(_:Link<Start>) {} fn main() {}",
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let (probe, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "pointer_storage_probe")
            .unwrap();
        assert!(super::may_contain_function_address(
            &hir.items,
            &typed.sigs,
            typed.sigs.fns[probe].params[0]
        ));
        assert!(!super::may_contain_function_address(
            &hir.items,
            &typed.sigs,
            flux_ty::Ty::new(flux_ty::TyKind::Int(flux_ty::IntTy::U32))
        ));
    }
}

#[cfg(test)]
mod atomic_tests {
    use super::*;

    #[test]
    fn pointer_bridge_requires_exact_thin_mutable_pointer_signatures() {
        let u8_ = Ty::int(flux_ty::IntTy::U8);
        let pointer = Ty::raw_ptr(Mutability::Mut, u8_);
        for ty in [
            pointer,
            Ty::raw_ptr(Mutability::Not, u8_),
            u8_,
            Ty::UNIT,
            Ty::raw_ptr(Mutability::Mut, Ty::STR),
            Ty::raw_ptr(Mutability::Mut, Ty::slice(u8_)),
            Ty::raw_ptr(
                Mutability::Mut,
                Ty::new(TyKind::Ref(Mutability::Not, Ty::STR)),
            ),
        ] {
            let supported =
                matches!(ty.kind(), TyKind::RawPtr(Mutability::Mut, t) if !t.is_unsized());
            for op in ["load", "store", "cas", "add", "sub", "fence"] {
                let address = Ty::raw_ptr(
                    if op == "load" {
                        Mutability::Not
                    } else {
                        Mutability::Mut
                    },
                    ty,
                );
                let sig = flux_ty::FnSig {
                    params: if op == "load" {
                        vec![address]
                    } else if op == "cas" {
                        vec![address, ty, ty]
                    } else {
                        vec![address, ty]
                    }
                    .into(),
                    ret: if op == "store" { Ty::UNIT } else { ty },
                };
                let valid = supported && matches!(op, "load" | "store" | "cas");
                assert_eq!(
                    valid_atomic_signature(op, AtomicWord::Ptr, &[ty], &sig),
                    valid
                );
                if valid {
                    let mut bad = sig.clone();
                    bad.ret = Ty::BOOL;
                    assert!(!valid_atomic_signature(op, AtomicWord::Ptr, &[ty], &bad));
                    let mut bad = sig.clone();
                    let mut params = bad.params.to_vec();
                    params[0] = pointer;
                    bad.params = params.into();
                    assert!(!valid_atomic_signature(op, AtomicWord::Ptr, &[ty], &bad));
                    assert!(!valid_atomic_signature(op, AtomicWord::Ptr, &[u8_], &sig));
                    assert!(!valid_atomic_signature(op, AtomicWord::Ptr, &[], &sig));
                }
            }
        }
        for name in [
            "__atomic_ptr_load_release",
            "__atomic_ptr_load_acqrel",
            "__atomic_ptr_store_acquire",
            "__atomic_ptr_store_acqrel",
            "__atomic_ptr_cas_release_relaxed",
            "__atomic_ptr_fence_acquire",
            "__atomic_ptr_store_unknown",
        ] {
            assert!(atomic_intrinsic(name).is_none(), "{name}");
        }
    }

    #[test]
    fn atomic_bridge_rejects_substitutions_and_invalid_intrinsic_orders() {
        for width in [
            flux_ty::IntTy::Usize,
            flux_ty::IntTy::U8,
            flux_ty::IntTy::U16,
            flux_ty::IntTy::U32,
        ] {
            let bridge = match width {
                flux_ty::IntTy::U8 => AtomicWord::U8,
                flux_ty::IntTy::U16 => AtomicWord::U16,
                flux_ty::IntTy::U32 => AtomicWord::U32,
                _ => AtomicWord::Usize,
            };
            let word = Ty::int(width);
            for ty in [
                word,
                Ty::int(flux_ty::IntTy::Usize),
                Ty::int(flux_ty::IntTy::U8),
                Ty::int(flux_ty::IntTy::I32),
                Ty::int(flux_ty::IntTy::U64),
                Ty::int(flux_ty::IntTy::U32),
                Ty::BOOL,
                Ty::UNIT,
                Ty::raw_ptr(Mutability::Mut, word),
            ] {
                for op in ["load", "store", "add", "sub", "cas"] {
                    let ptr = Ty::raw_ptr(
                        if op == "load" {
                            Mutability::Not
                        } else {
                            Mutability::Mut
                        },
                        ty,
                    );
                    let params = match op {
                        "load" => vec![ptr],
                        "cas" => vec![ptr, ty, ty],
                        _ => vec![ptr, ty],
                    };
                    let sig = flux_ty::FnSig {
                        params: params.into(),
                        ret: if op == "store" { Ty::UNIT } else { ty },
                    };
                    let supported = width == flux_ty::IntTy::Usize
                        || matches!(op, "load" | "store" | "cas")
                        || width == flux_ty::IntTy::U32 && op == "add";
                    assert_eq!(
                        valid_atomic_signature(op, bridge, &[ty], &sig),
                        ty == word && supported
                    );
                    if ty == word && supported {
                        let mut bad = sig.clone();
                        bad.ret = Ty::BOOL;
                        assert!(!valid_atomic_signature(op, bridge, &[ty], &bad));
                        let mut bad = sig.clone();
                        let mut params = bad.params.to_vec();
                        params[0] = Ty::raw_ptr(
                            if op == "load" {
                                Mutability::Mut
                            } else {
                                Mutability::Not
                            },
                            word,
                        );
                        bad.params = params.into();
                        assert!(!valid_atomic_signature(op, bridge, &[ty], &bad));
                    }
                    assert!(!valid_atomic_signature(op, bridge, &[], &sig));
                }
            }
        }
        for name in [
            "__atomic_usize_load_release",
            "__atomic_usize_load_acqrel",
            "__atomic_usize_store_acquire",
            "__atomic_usize_store_acqrel",
            "__atomic_usize_fence_relaxed",
            "__atomic_usize_cas_unknown",
            "__atomic_u32_load_release",
            "__atomic_u32_load_acqrel",
            "__atomic_u32_store_acquire",
            "__atomic_u32_store_acqrel",
            "__atomic_u32_add_unknown",
            "__atomic_u64_add_relaxed",
            "__atomic_ptr_add_relaxed",
            "__atomic_u32_sub_relaxed",
            "__atomic_u32_cas_unknown",
            "__atomic_u32_fence_acquire",
        ] {
            assert!(atomic_intrinsic(name).is_none(), "{name}");
        }
        let word = Ty::int(flux_ty::IntTy::Usize);
        let bad = flux_ty::FnSig {
            params: vec![Ty::raw_ptr(Mutability::Mut, word), word].into(),
            ret: Ty::BOOL,
        };
        assert!(!valid_atomic_signature(
            "add",
            AtomicWord::Usize,
            &[word],
            &bad
        ));
    }
}
