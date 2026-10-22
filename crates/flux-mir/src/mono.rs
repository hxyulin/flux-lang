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
use std::collections::HashMap;

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
    pub reified: std::collections::HashSet<Instance>,
}

/// [`collect`], with the instances whose address is taken.
pub fn collect_all(hir: &Hir, sigs: &Signatures, mir: &FileMir) -> Result<Collected, Diagnostic> {
    let mut reified = std::collections::HashSet::new();
    let mut instances = Vec::new();
    let mut depths: HashMap<Instance, u32> = HashMap::new();
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
            let mut pointers = Vec::new();
            for statement in &data.statements {
                if let StatementKind::Assign(_, rvalue) = &statement.kind {
                    pointers.extend(rvalue.operands().into_iter().cloned());
                    // A vtable's methods, for a value made a trait object.
                    if let Rvalue::Unsize(op, to) = rvalue {
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
                            let entries =
                                crate::vtable::entries(&hir.items, sigs, object, concrete);
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
            }
            pointers.extend(data.terminator.kind.operands().into_iter().cloned());
            for op in pointers {
                if let Operand::Const(Constant::Fn { function, args, .. }) = op {
                    let instance = Instance {
                        function,
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
            let callees = match &data.terminator.kind {
                TerminatorKind::Call {
                    func, generic_args, ..
                } => vec![Instance {
                    function: *func,
                    args: instance_args(&hir.items, sigs, generic_args, caller_args),
                }],
                // What a failed check calls.
                TerminatorKind::Assert { msg, .. } => items
                    .lang_fns
                    .get(&Symbol::intern(msg.lang_fn()))
                    .map(|&function| Instance {
                        function,
                        args: &[],
                    })
                    .into_iter()
                    .collect(),
                TerminatorKind::Drop { place, .. } => {
                    let ty = body.place_ty(place, sigs);
                    let ty = flux_ty::traits::instance_ty(&hir.items, sigs, ty, caller_args);
                    let mut glue = Vec::new();
                    drop_glue(&hir.items, sigs, place.clone(), ty, &mut glue);
                    let mut types = Vec::new();
                    glue_types(&glue, &mut types);
                    let drop = drop_fn(&hir.items);
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
                _ => continue,
            };
            for callee in callees {
                let callee = flux_ty::traits::resolve_instance(&hir.items, sigs, callee);
                // Through a vtable, whose methods are collected where
                // values become trait objects.
                if crate::vtable::virtual_call(&hir.items, callee).is_some() {
                    continue;
                }
                if hir.items.functions[callee.function].intrinsic {
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
        _ => None,
    }
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
        Some((body, args)) => instance_body(hir, sigs, body, args),
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
    // A closure is cloned by a copy: it is `Clone` only where `Copy`.
    if matches!(
        instance.args.first().map(|t| t.kind()),
        Some(TyKind::Closure(..))
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
        // A closure, `Copy`: a copy of it.
        TyKind::Closure(..) => (array, 0),
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
    body: &'b Body,
    args: &[Ty],
) -> Cow<'b, Body> {
    let mut body = if args.is_empty() && !mentions_opaques(body) {
        Cow::Borrowed(body)
    } else {
        Cow::Owned(subst_body(hir, sigs, body, args))
    };
    let drops = body
        .blocks
        .values()
        .any(|data| matches!(data.terminator.kind, TerminatorKind::Drop { .. }));
    if drops {
        expand_drops(&hir.items, sigs, body.to_mut());
    }
    let intrinsics = body.blocks.values().any(|data| {
        matches!(data.terminator.kind, TerminatorKind::Call { func, .. }
            if hir.items.functions[func].intrinsic)
    });
    if intrinsics {
        expand_intrinsics(&hir.items, sigs, body.to_mut());
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
/// - `subslice(s, start, end)`, `subslice_mut(s, start, end)`:
///   `&s[start..end]`, unchecked;
/// - `slice_from_raw_parts(data, len)`: `data` as a pointer to an empty
///   array, unsized, then its first `len` elements, unchecked;
/// - `c_str_from_ptr(p)`: `p` as the `&CStr` it is.
pub fn expand_intrinsics(items: &ItemTree, sigs: &Signatures, body: &mut Body) {
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
        if data.name.as_str() == "caller_location" {
            // Code generation's: where the call is.
            continue;
        }
        if data.name.as_str() == "uninit" {
            // Nothing: its memory is written before it is read.
            body.blocks[block].terminator.kind = goto;
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
            ("read", [src]) => {
                let src = pointer(body, src);
                (dest, Rvalue::Use(Operand::Copy(src.deref())))
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
            ("slice_from_raw_parts", [data, len]) => {
                let slice_ref = body.place_ty(&dest, sigs);
                let elem = match slice_ref.kind() {
                    TyKind::Ref(_, slice) => slice.sequence_elem().unwrap_or(Ty::ERROR),
                    _ => Ty::ERROR,
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
                let array = Ty::new(TyKind::RawPtr(Mutability::Not, Ty::array(elem, 0)));
                let raw_slice = Ty::new(TyKind::RawPtr(
                    Mutability::Not,
                    Ty::new(TyKind::Slice(elem)),
                ));
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
            projection: p
                .projection
                .iter()
                .map(|&e| match e {
                    crate::ProjectionElem::Index(l) if l == from => {
                        crate::ProjectionElem::Index(to)
                    }
                    e => e,
                })
                .collect(),
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
        if let Operand::Const(Constant::FnItem(ty)) = op {
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
