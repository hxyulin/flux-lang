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
}

/// [`collect`], with the instances whose address is taken.
pub fn collect_all(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Collected, Diagnostic> {
    let mut reified = FxHashSet::default();
    let mut instances = Vec::new();
    let mut depths: FxHashMap<Instance, u32> = FxHashMap::default();
    let items = &hir.items;
    let root = items.module_of(items.main_crate());
    let main = mir.bodies.iter().map(|(id, _)| id).find(|&id| {
        let f = &items.functions[id];
        f.name.as_str() == "main" && f.loc.module == root && f.owner.is_none()
    });
    if let Some(function) = main {
        let instance = Instance {
            function,
            args: &[],
        };
        depths.insert(instance, 0);
        instances.push(instance);
        // What the entry point calls after `main`.
        if let Some(&flush) = items.lang_fns.get(&Symbol::intern("flush_stdout"))
            && mir.bodies.get(flush).is_some()
        {
            let instance = Instance {
                function: flush,
                args: &[],
            };
            depths.insert(instance, 0);
            instances.push(instance);
        }
    }
    let mut next = 0;
    while let Some(&caller) = instances.get(next) {
        next += 1;
        let depth = depths[&caller];
        let shim;
        let (body, caller_args) = match mir.instance_body(&hir.items, caller) {
            Some(found) => found,
            None => {
                shim = clone_shim(&hir.items, sigs, caller).expect("instances have bodies");
                (&shim, caller.args)
            }
        };
        for data in body.blocks.values() {
            // Functions whose address is taken.
            for statement in &data.statements {
                // A vtable's methods, for a value made a trait object.
                if let StatementKind::Assign(_, Rvalue::Unsize(op, to)) = &statement.kind {
                    let from = flux_ty::traits::instance_ty(
                        &hir.items,
                        sigs,
                        body.operand_ty(op, sigs),
                        caller_args,
                    );
                    let to = flux_ty::traits::instance_ty(&hir.items, sigs, *to, caller_args);
                    let mut vtables = Vec::new();
                    if let (Some(concrete), Some(object)) = (pointee(from), pointee(to))
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
                                            || is_clone_shim(&hir.items, instance);
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
                if let Operand::Const(Constant::Fn { function, args, .. }) = op {
                    let instance = Instance {
                        function: *function,
                        args: instance_args(&hir.items, sigs, args, caller_args),
                    };
                    let instance = flux_ty::traits::resolve_instance(&hir.items, sigs, instance);
                    reified.insert(instance);
                    let has_body = mir.instance_body(&hir.items, instance).is_some()
                        || is_clone_shim(&hir.items, instance);
                    if has_body && !depths.contains_key(&instance) {
                        depths.insert(instance, depth + 1);
                        instances.push(instance);
                    }
                }
            }
            let single;
            let callees: Cow<'_, [Instance]> = match &data.terminator.kind {
                // What dropping in place drops.
                TerminatorKind::Call {
                    func, generic_args, ..
                } if hir.items.functions[*func].intrinsic
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
                    let ty = flux_ty::traits::instance_ty(&hir.items, sigs, ty, caller_args);
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
                let callee = flux_ty::traits::resolve_instance(&hir.items, sigs, callee);
                // Through a vtable, whose methods are collected where
                // values become trait objects.
                if crate::vtable::virtual_call(&hir.items, callee).is_some() {
                    continue;
                }
                let callee_data = &hir.items.functions[callee.function];
                let name = callee_data.name;
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

                if hir.items.functions[callee.function].intrinsic {
                    if let Some((op, _, word)) =
                        atomic_intrinsic(hir.items.functions[callee.function].name.as_str())
                    {
                        let sig = flux_ty::traits::instance_sig(
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
                                    | flux_ty::IntTy::Usize
                            ))
                        )
                    {
                        let mut d = Diagnostic::new(
                            data.terminator.range,
                            "read_unaligned supports only u8, u16, u32, u64 and usize yet",
                        );
                        d.file = hir.items.functions[caller.function].loc.file;
                        return Err(d);
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
                    && !is_clone_shim(&hir.items, callee)
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
    Ok(Collected { instances, reified })
}

/// What a pointer type points to.
fn pointee(ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => Some(t),
        _ => flux_ty::box_pointee(ty),
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
        .map(|&a| flux_ty::traits::instance_ty(items, sigs, a, caller_args))
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
/// substituted and its drops expanded, or the compiler's own for
/// `Clone::clone` of an array.
pub fn body_of<'b>(
    hir: &Hir,
    sigs: &Signatures,
    mir: &'b FileMir,
    instance: Instance,
) -> Cow<'b, Body> {
    match mir.instance_body(&hir.items, instance) {
        Some((body, args)) => instance_body(hir, sigs, &mir.discriminants, body, args),
        None => Cow::Owned(clone_shim(&hir.items, sigs, instance).expect("instances have bodies")),
    }
}

/// The method of trait `lang` (a lang item's name) called `name`.
fn lang_method(items: &ItemTree, lang: &str, name: &str) -> Option<FunctionId> {
    let t = *items.lang_traits.get(&flux_intern::Symbol::intern(lang))?;
    items.traits[t].function(items, flux_intern::Symbol::intern(name))
}

/// Whether `instance` is `Clone::clone` or `PartialEq::eq` for an array,
/// which the compiler implements, as rustc's `core` does with const
/// generics.
fn is_clone_shim(items: &ItemTree, instance: Instance) -> bool {
    if is_fn_shim(items, instance) {
        return true;
    }
    // Functions and pointers clone by copying, as do `Copy` closures.
    if matches!(
        instance.args.first().map(|t| t.kind()),
        Some(TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_))
    ) && Some(instance.function) == lang_method(items, "clone", "clone")
    {
        return true;
    }
    false
}

/// `Clone::clone` for an array of `T`: a copy if `T` is `Copy`, else
/// `T::clone` of each element in turn; or `PartialEq::eq`, which
/// compares each element in turn, until one differs.
fn clone_shim(items: &ItemTree, sigs: &Signatures, instance: Instance) -> Option<Body> {
    if !is_clone_shim(items, instance) {
        return None;
    }
    if is_fn_shim(items, instance) {
        return Some(fn_shim(items, sigs, instance));
    }
    let array = instance.args[0];
    let (elem, len) = match array.kind() {
        // A function item, pointer or `Copy` closure: a copy of it.
        TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_) => (array, 0),
        _ => return None,
    };
    let usize = Ty::int(flux_ty::IntTy::Usize);
    let mut locals = flux_arena::Arena::new();
    let local =
        |locals: &mut flux_arena::Arena<LocalDecl>, ty, kind| locals.alloc(LocalDecl { ty, kind });
    let ret = local(&mut locals, array, LocalKind::Return);
    let this = local(
        &mut locals,
        Ty::new(TyKind::Ref(Mutability::Not, array)),
        LocalKind::Param,
    );
    let range = flux_syntax::TextRange::default();
    let statement = |place: Local, rvalue| Statement {
        kind: StatementKind::Assign(place.into(), rvalue),
        range,
    };
    let terminator = |kind| Terminator { kind, range };
    let mut blocks = flux_arena::Arena::new();
    if flux_ty::traits::is_copy(items, sigs, &[], elem) {
        blocks.alloc(BasicBlockData {
            statements: vec![statement(
                ret,
                Rvalue::Use(Operand::Copy(Place::from(this).deref())),
            )],
            terminator: terminator(TerminatorKind::Return),
        });
    } else {
        let counter = local(&mut locals, usize, LocalKind::Temp);
        let within = local(&mut locals, Ty::BOOL, LocalKind::Temp);
        let element = local(
            &mut locals,
            Ty::new(TyKind::Ref(Mutability::Not, elem)),
            LocalKind::Temp,
        );
        let int =
            |value: u64| Operand::Const(Constant::int(i128::from(value), flux_ty::IntTy::Usize));
        let (head, body, step, exit) = (
            BasicBlock::from_raw(1),
            BasicBlock::from_raw(2),
            BasicBlock::from_raw(3),
            BasicBlock::from_raw(4),
        );
        blocks.alloc(BasicBlockData {
            statements: vec![statement(counter, Rvalue::Use(int(0)))],
            terminator: terminator(TerminatorKind::Goto(head)),
        });
        blocks.alloc(BasicBlockData {
            statements: vec![statement(
                within,
                Rvalue::BinaryOp(crate::BinOp::Lt, Operand::Copy(counter.into()), int(len)),
            )],
            terminator: terminator(TerminatorKind::SwitchInt {
                discr: Operand::Copy(within.into()),
                targets: vec![(0, exit)],
                otherwise: body,
                variants: None,
            }),
        });
        blocks.alloc(BasicBlockData {
            statements: vec![statement(
                element,
                Rvalue::Ref {
                    mutability: Mutability::Not,
                    place: Place::from(this).deref().index(counter),
                    two_phase: false,
                },
            )],
            terminator: terminator(TerminatorKind::Call {
                func: instance.function,
                generic_args: Ty::list(&[elem]),
                args: vec![Operand::Move(element.into())],
                dest: Place::from(ret).index(counter),
                target: Some(step),
            }),
        });
        blocks.alloc(BasicBlockData {
            statements: vec![statement(
                counter,
                Rvalue::BinaryOp(crate::BinOp::Add, Operand::Copy(counter.into()), int(1)),
            )],
            terminator: terminator(TerminatorKind::Goto(head)),
        });
        blocks.alloc(BasicBlockData {
            statements: Vec::new(),
            terminator: terminator(TerminatorKind::Return),
        });
    }
    Some(Body {
        locals,
        call_regions: flux_arena::ArenaMap::new(),
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
        promoted: Vec::new(),
        bugs: Vec::new(),
        fn_spans: Default::default(),
    })
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
    let sig = flux_ty::traits::instance_sig(items, sigs, instance.function, instance.args);
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
            if hir.items.functions[func].intrinsic)
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
/// - `c_str_from_ptr(p)`: `p` as the `&CStr` it is.
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
        if !data.intrinsic {
            continue;
        }
        let goto = match target {
            Some(target) => TerminatorKind::Goto(target),
            None => TerminatorKind::Unreachable,
        };
        if matches!(data.name.as_str(), "caller_location" | "read_unaligned")
            || is_neon_intrinsic(data.name.as_str())
            || atomic_intrinsic(data.name.as_str()).is_some()
        {
            // Code generation's: where the call is.
            continue;
        }
        if data.name.as_str() == "unreachable_unchecked" {
            body.blocks[block].terminator.kind = TerminatorKind::Unreachable;
            continue;
        }
        if data.name.as_str() == "uninit" {
            // Nothing: its memory is written before it is read.
            body.blocks[block].terminator.kind = goto;
            continue;
        }
        // Of a trait object: through its vtable, code generation's.
        let arg_ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
        let name = data.name.as_str();
        if matches!(name, "size_of_val" | "align_of_val" | "drop_in_place") && arg_ty.is_dyn() {
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
        let (place, rvalue) = match (data.name.as_str(), args.as_slice()) {
            ("is_copy", []) => {
                let ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
                let copy = flux_ty::traits::is_copy(items, sigs, &[], ty);
                (dest, Rvalue::Use(Operand::Const(Constant::Bool(copy))))
            }
            ("needs_drop", []) => {
                let ty = generic_args.first().copied().unwrap_or(Ty::ERROR);
                let needs = flux_ty::traits::needs_drop(items, sigs, &[], ty);
                (dest, Rvalue::Use(Operand::Const(Constant::Bool(needs))))
            }
            (name @ ("size_of_val" | "align_of_val"), [ptr]) if arg_ty.is_unsized() => {
                // A slice's or `str`'s: its length times its element's.
                let elem = arg_ty
                    .sequence_elem()
                    .unwrap_or(Ty::int(flux_ty::IntTy::U8));
                let layout = layouts.of(elem);
                let usize = flux_ty::IntTy::Usize;
                if name == "align_of_val" {
                    let constant = Constant::Int {
                        bits: u128::from(layout.align),
                        ty: usize,
                    };
                    (dest, Rvalue::Use(Operand::Const(constant)))
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
                    let size = Operand::Const(Constant::Int {
                        bits: u128::from(layout.size),
                        ty: usize,
                    });
                    (
                        dest,
                        Rvalue::BinaryOp(crate::BinOp::Mul, Operand::Move(len.into()), size),
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
            ("c_str_from_ptr", [ptr]) => {
                let ty = body.place_ty(&dest, sigs);
                (dest, Rvalue::Cast(ptr.clone(), ty))
            }
            // `data` as a pointer to an empty array, unsized, as a
            // reference, then its first `len` elements.
            ("slice_from_raw_parts" | "slice_from_raw_parts_mut", [data, len]) => {
                let slice_ref = body.place_ty(&dest, sigs);
                let (m, elem) = match slice_ref.kind() {
                    TyKind::Ref(m, slice) => (m, slice.sequence_elem().unwrap_or(Ty::ERROR)),
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
        && !flux_ty::traits::needs_drop(items, sigs, &[], elem)
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
    /// For an enum at `place`, the glue of the variant it holds.
    Variants(Place, StructId, Vec<Vec<Glue>>),
    /// For an array of `len` elements, the glue of each, in order, at
    /// places indexed by `index`, a stand-in for the loop's counter.
    Elements {
        index: Local,
        len: u64,
        glue: Vec<Glue>,
    },
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
            Glue::Elements { glue, .. } => glue_types(glue, out),
        }
    }
}

/// The glue of a `ty` at `place`: a struct's own `Drop` impl, then its
/// fields', in order; an enum's, then the fields' of the variant it
/// holds.
fn drop_glue(items: &ItemTree, sigs: &Signatures, place: Place, ty: Ty, out: &mut Vec<Glue>) {
    match ty.kind() {
        TyKind::Adt(adt, _) if flux_ty::is_lang_struct(items, adt.strukt, "manually_drop") => {
            return;
        }
        TyKind::Adt(adt, _) => {
            if flux_ty::traits::drop_impl(items, sigs, adt.strukt).is_some() {
                out.push(Glue::Drop(place.clone(), ty));
            }
            if let Some(variants) = sigs.variants(ty) {
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
                out.push(Glue::Elements { index, len, glue });
            }
            return;
        }
        _ => return,
    }
    for (i, &field) in sigs.fields(ty).unwrap_or_default().iter().enumerate() {
        drop_glue(items, sigs, place.field(i as u32), field, out);
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
            Glue::Drop(place, ty) => {
                let pointer = self.body.locals.alloc(LocalDecl {
                    ty: Ty::new(TyKind::Ref(Mutability::Mut, *ty)),
                    kind: LocalKind::Temp,
                });
                let borrow = Statement {
                    kind: StatementKind::Assign(
                        pointer.into(),
                        Rvalue::Ref {
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
                        Rvalue::BinaryOp(
                            crate::BinOp::Lt,
                            Operand::Copy(counter.into()),
                            int(*len),
                        ),
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
                (
                    vec![assign(counter, Rvalue::Use(int(0)), self.range)],
                    TerminatorKind::Goto(head),
                )
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
                len: *len,
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
    let mut body = body.clone();
    let items = &hir.items;
    let subst = |ty: Ty| flux_ty::traits::instance_ty(items, sigs, ty, args);
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
                Rvalue::Use(op) | Rvalue::UnaryOp(_, op) => operand(op),
                Rvalue::Cast(op, ty) => {
                    operand(op);
                    *ty = subst(*ty);
                }
                Rvalue::BinaryOp(_, l, r) => {
                    operand(l);
                    operand(r);
                }
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
                Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Len { .. } => {}
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

/// Core's bounded NEON intrinsics are emitted directly as Keel operations.
pub fn is_neon_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "__neon_vld1q_u8"
            | "__neon_vdupq_n_u8"
            | "__neon_vceqq_u8"
            | "__neon_vandq_u8"
            | "__neon_vorrq_u8"
            | "__neon_vgetq_lane_u64"
            | "__neon_vget_lane_u64"
            | "__neon_vshrn_n_u16"
            | "__neon_vpmaxq_u8"
    )
}

/// Scalar storage identity for the private native atomic bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicWord {
    Usize,
    U32,
    Ptr,
}

/// Exact bridge names and fixed valid orderings; no runtime ordering crosses this boundary.
pub fn atomic_intrinsic(name: &str) -> Option<(&str, &str, AtomicWord)> {
    let (rest, word) = if let Some(rest) = name.strip_prefix("__atomic_usize_") {
        (rest, AtomicWord::Usize)
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
            AtomicWord::U32 => matches!(op, "load" | "store" | "add"),
            AtomicWord::Ptr => matches!(op, "load" | "store"),
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
        AtomicWord::U32 if matches!(op, "load" | "store" | "add") => Ty::int(flux_ty::IntTy::U32),
        AtomicWord::Ptr if matches!(op, "load" | "store") => {
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
                    } else {
                        vec![address, ty]
                    }
                    .into(),
                    ret: if op == "store" { Ty::UNIT } else { ty },
                };
                let valid = supported && matches!(op, "load" | "store");
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
            "__atomic_ptr_cas_seqcst",
            "__atomic_ptr_fence_acquire",
            "__atomic_ptr_store_unknown",
        ] {
            assert!(atomic_intrinsic(name).is_none(), "{name}");
        }
    }

    #[test]
    fn atomic_bridge_rejects_substitutions_and_invalid_intrinsic_orders() {
        for width in [flux_ty::IntTy::Usize, flux_ty::IntTy::U32] {
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
                    let supported =
                        width == flux_ty::IntTy::Usize || matches!(op, "load" | "store" | "add");
                    assert_eq!(
                        valid_atomic_signature(
                            op,
                            if width == flux_ty::IntTy::U32 {
                                AtomicWord::U32
                            } else {
                                AtomicWord::Usize
                            },
                            &[ty],
                            &sig
                        ),
                        ty == word && supported
                    );
                    if ty == word && supported {
                        let mut bad = sig.clone();
                        bad.ret = Ty::BOOL;
                        assert!(!valid_atomic_signature(
                            op,
                            if width == flux_ty::IntTy::U32 {
                                AtomicWord::U32
                            } else {
                                AtomicWord::Usize
                            },
                            &[ty],
                            &bad
                        ));
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
                        assert!(!valid_atomic_signature(
                            op,
                            if width == flux_ty::IntTy::U32 {
                                AtomicWord::U32
                            } else {
                                AtomicWord::Usize
                            },
                            &[ty],
                            &bad
                        ));
                    }
                    assert!(!valid_atomic_signature(
                        op,
                        if width == flux_ty::IntTy::U32 {
                            AtomicWord::U32
                        } else {
                            AtomicWord::Usize
                        },
                        &[],
                        &sig
                    ));
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
            "__atomic_u32_cas_relaxed",
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
