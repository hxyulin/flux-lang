//! Stable coroutine frames, resumption states and checked cancellation paths.
use crate::{
    AggregateKind, AssertMessage, BasicBlock, BasicBlockData, Body, Constant, FileMir, Local,
    LocalDecl, LocalKind, Operand, Place, ProjectionElem, Rvalue, Statement, StatementKind,
    Terminator, TerminatorKind,
};
use flux_hir::{Hir, Intrinsic, ItemTree};
use flux_intern::Symbol;
use flux_ty::{ClosureArgs, ClosureKind, Instance, IntTy, Mutability, Signatures, Ty, TyKind};

fn lang(items: &ItemTree, name: &str) -> flux_hir::FunctionId {
    items.lang_fns[&Symbol::intern(name)]
}
fn local(body: &mut Body, ty: Ty, kind: LocalKind) -> Local {
    body.locals.alloc(LocalDecl { ty, kind })
}
fn block(body: &mut Body, statements: Vec<Statement>, kind: TerminatorKind) -> BasicBlock {
    body.blocks.alloc(BasicBlockData {
        statements,
        terminator: Terminator {
            unwind: crate::UnwindAction::Unreachable,
            kind,
            range: Default::default(),
        },
    })
}
fn assign(place: Place, value: Rvalue) -> Statement {
    Statement {
        kind: StatementKind::Assign(place, value),
        range: Default::default(),
    }
}
fn number(value: u64) -> Operand {
    Operand::Const(Constant::int(value as i128, IntTy::Usize))
}
fn frame_place(pointer: Local, old: Local) -> Place {
    Place::from(pointer).deref().field((old.index() as u32) + 1)
}
fn state(pointer: Local) -> Place {
    Place::from(pointer).deref().field(0)
}
fn go(target: BasicBlock) -> TerminatorKind {
    TerminatorKind::Goto(target)
}

struct Source {
    body: Body,
    frame: Ty,
    awaits: Vec<(BasicBlock, BasicBlock)>,
}
fn source(hir: &Hir, sigs: &Signatures, mir: &FileMir, capture: Ty) -> Source {
    let TyKind::Closure(id, args) = capture.kind() else {
        panic!("coroutine capture is a closure");
    };
    let template = &mir.closures[&(id, ClosureKind::FnOnce)];
    assert!(
        !template.has_unwind(),
        "unwind coroutine lowering is not implemented yet"
    );
    let body = crate::mono::subst_body(hir, sigs, template, ClosureArgs::split(args).parent);
    let mut fields = vec![Ty::int(IntTy::Usize)];
    fields.extend(body.locals.values().map(|local| local.ty));
    let awaits = body
        .blocks
        .iter()
        .filter_map(|(id, data)| {
            let TerminatorKind::Call {
                func,
                target: Some(target),
                ..
            } = &data.terminator.kind
            else {
                return None;
            };
            if hir.items.functions[*func].intrinsic != Some(Intrinsic::CoroutineAwait) {
                return None;
            }
            let TerminatorKind::SwitchInt {
                discr: Operand::Const(Constant::Bool(false)),
                targets,
                ..
            } = &body.blocks[*target].terminator.kind
            else {
                panic!("await retains its cleanup fork");
            };
            assert_eq!(targets.len(), 1, "one await cleanup arm");
            Some((id, targets[0].1))
        })
        .collect();
    Source {
        body,
        frame: Ty::tuple(&fields),
        awaits,
    }
}

pub(crate) fn body_of(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    instance: Instance,
) -> Option<Body> {
    let operation = hir.items.functions[instance.function].intrinsic?;
    if !matches!(
        operation,
        Intrinsic::CoroutineNew | Intrinsic::CoroutinePoll | Intrinsic::CoroutineDrop
    ) {
        return None;
    }
    let capture = instance.args[0];
    let source = source(hir, sigs, mir, capture);
    Some(if operation == Intrinsic::CoroutineNew {
        construct(hir, sigs, mir, instance, &source)
    } else {
        resume(
            hir,
            sigs,
            instance,
            &source,
            operation == Intrinsic::CoroutineDrop,
        )
    })
}

fn construct(
    hir: &Hir,
    sigs: &Signatures,
    mir: &FileMir,
    instance: Instance,
    source: &Source,
) -> Body {
    let signature =
        flux_solve::traits::instance_sig(&hir.items, sigs, instance.function, instance.args);
    let mut body = Body::empty(Some(instance.function));
    let ret = local(&mut body, signature.ret, LocalKind::Return);
    let capture = local(&mut body, instance.args[0], LocalKind::Param);
    body.param_count = 1;
    let raw = local(
        &mut body,
        Ty::raw_ptr(Mutability::Mut, Ty::int(IntTy::U8)),
        LocalKind::Temp,
    );
    let pointer = local(
        &mut body,
        Ty::raw_ptr(Mutability::Mut, source.frame),
        LocalKind::Temp,
    );
    let layout = crate::layout::Layouts::new(&hir.items, sigs, &mir.discriminants).of(source.frame);
    block(
        &mut body,
        vec![],
        TerminatorKind::Call {
            func: lang(&hir.items, "coroutine_alloc"),
            generic_args: &[],
            args: vec![number(layout.size), number(layout.align)],
            dest: raw.into(),
            target: Some(BasicBlock::from_raw(1)),
        },
    );
    let TyKind::Adt(coroutine, args) = signature.ret.kind() else {
        unreachable!();
    };
    let marker_ty = flux_solve::field_types(sigs, coroutine.strukt, args)[1];
    let marker = local(&mut body, marker_ty, LocalKind::Temp);
    let TyKind::Adt(marker_def, _) = marker_ty.kind() else {
        unreachable!();
    };
    let mut statements = vec![
        assign(
            pointer.into(),
            Rvalue::Cast(Operand::Copy(raw.into()), body.locals[pointer].ty),
        ),
        assign(state(pointer), Rvalue::Use(number(0))),
        assign(
            frame_place(pointer, Local::from_raw(1)),
            Rvalue::Use(Operand::Move(capture.into())),
        ),
        assign(
            frame_place(pointer, Local::from_raw(2)),
            Rvalue::Use(Operand::Const(Constant::Unit)),
        ),
        assign(
            marker.into(),
            Rvalue::Aggregate(AggregateKind::Struct(marker_def.strukt), vec![]),
        ),
    ];
    let pinned_ty = flux_solve::field_types(sigs, coroutine.strukt, args)[2];
    let pinned = local(&mut body, pinned_ty, LocalKind::Temp);
    let TyKind::Adt(pinned_def, _) = pinned_ty.kind() else {
        unreachable!();
    };
    statements.push(assign(
        pinned.into(),
        Rvalue::Aggregate(AggregateKind::Struct(pinned_def.strukt), vec![]),
    ));
    statements.push(assign(
        ret.into(),
        Rvalue::Aggregate(
            AggregateKind::Struct(coroutine.strukt),
            vec![
                Operand::Copy(raw.into()),
                Operand::Move(marker.into()),
                Operand::Move(pinned.into()),
            ],
        ),
    ));
    block(&mut body, statements, TerminatorKind::Return);
    body
}

/// Rebase every place into the stable allocation. Index projections still
/// name MIR locals, so snapshot their frame fields immediately before use.
struct Rebase {
    pointer: Local,
    indices: Vec<Local>,
}
impl Rebase {
    fn place(&self, place: &mut Place, statements: &mut Vec<Statement>) {
        let mut projection = vec![
            ProjectionElem::Deref,
            ProjectionElem::Field((place.local.index() as u32) + 1),
        ];
        for &element in place.projection.iter() {
            projection.push(if let ProjectionElem::Index(index) = element {
                let scratch = self.indices[index.index()];
                statements.push(assign(
                    scratch.into(),
                    Rvalue::Use(Operand::Copy(frame_place(self.pointer, index))),
                ));
                ProjectionElem::Index(scratch)
            } else {
                element
            });
        }
        place.local = self.pointer;
        place.projection = projection.into_iter().collect();
    }
    fn operand(&self, operand: &mut Operand, statements: &mut Vec<Statement>) {
        if let Operand::Move(place) | Operand::Copy(place) = operand {
            self.place(place, statements);
        }
    }
    fn value(&self, value: &mut Rvalue, statements: &mut Vec<Statement>) {
        match value {
            Rvalue::Use(op)
            | Rvalue::UnaryOp(_, op)
            | Rvalue::Cast(op, _)
            | Rvalue::Repeat(op, _)
            | Rvalue::Unsize(op, _) => self.operand(op, statements),
            Rvalue::WithAddr(a, b) | Rvalue::BinaryOp(_, a, b) => {
                self.operand(a, statements);
                self.operand(b, statements);
            }
            Rvalue::InlineAsm { inputs, .. } | Rvalue::Aggregate(_, inputs) => {
                for input in inputs {
                    self.operand(input, statements);
                }
            }
            Rvalue::Discriminant(place)
            | Rvalue::DiscriminantForDrop(place)
            | Rvalue::Ref { place, .. }
            | Rvalue::Len { place, .. } => self.place(place, statements),
            Rvalue::Subslice { slice, start, end } => {
                for op in [slice, start, end] {
                    self.operand(op, statements);
                }
            }
            Rvalue::OffsetOf(..) => {}
        }
    }
    fn terminator(&self, kind: &mut TerminatorKind, statements: &mut Vec<Statement>) {
        match kind {
            TerminatorKind::Call { args, dest, .. } => {
                for arg in args {
                    self.operand(arg, statements);
                }
                self.place(dest, statements);
            }
            TerminatorKind::CallIndirect {
                callee, args, dest, ..
            } => {
                self.operand(callee, statements);
                for arg in args {
                    self.operand(arg, statements);
                }
                self.place(dest, statements);
            }
            TerminatorKind::Drop { place, .. } => self.place(place, statements),
            TerminatorKind::SwitchInt { discr, .. } => self.operand(discr, statements),
            TerminatorKind::Assert { cond, msg, .. } => {
                self.operand(cond, statements);
                match msg {
                    AssertMessage::DivisionByZero(a) | AssertMessage::RemainderByZero(a) => {
                        self.operand(a, statements)
                    }
                    AssertMessage::DivisionOverflow(a, b)
                    | AssertMessage::RemainderOverflow(a, b)
                    | AssertMessage::BoundsCheck { len: a, index: b } => {
                        self.operand(a, statements);
                        self.operand(b, statements);
                    }
                }
            }
            TerminatorKind::Goto(_)
            | TerminatorKind::Return
            | TerminatorKind::UnwindResume
            | TerminatorKind::Unreachable => {}
        }
        targets(kind, |block| {
            BasicBlock::from_raw((block.index() as u32) + 1)
        });
    }
}
fn targets(kind: &mut TerminatorKind, map: impl Fn(BasicBlock) -> BasicBlock) {
    match kind {
        TerminatorKind::Goto(target)
        | TerminatorKind::Drop { target, .. }
        | TerminatorKind::Assert { target, .. } => *target = map(*target),
        TerminatorKind::Call { target, .. } | TerminatorKind::CallIndirect { target, .. } => {
            if let Some(target) = target {
                *target = map(*target);
            }
        }
        TerminatorKind::SwitchInt {
            targets, otherwise, ..
        } => {
            for (_, target) in targets {
                *target = map(*target);
            }
            *otherwise = map(*otherwise);
        }
        TerminatorKind::Return | TerminatorKind::UnwindResume | TerminatorKind::Unreachable => {}
    }
}

fn resume(
    hir: &Hir,
    sigs: &Signatures,
    instance: Instance,
    source: &Source,
    dropping: bool,
) -> Body {
    let signature =
        flux_solve::traits::instance_sig(&hir.items, sigs, instance.function, instance.args);
    let mut body = Body::empty(source.body.owner);
    local(&mut body, signature.ret, LocalKind::Return);
    for &param in &signature.params {
        local(&mut body, param, LocalKind::Param);
    }
    body.param_count = signature.params.len();
    let pointer = local(
        &mut body,
        Ty::raw_ptr(Mutability::Mut, source.frame),
        LocalKind::Temp,
    );
    let indices = source
        .body
        .locals
        .values()
        .map(|decl| local(&mut body, decl.ty, LocalKind::Temp))
        .collect();
    let rebase = Rebase { pointer, indices };
    let pointer_ty = body.locals[pointer].ty;
    block(
        &mut body,
        vec![assign(
            pointer.into(),
            Rvalue::Cast(Operand::Copy(Local::from_raw(1).into()), pointer_ty),
        )],
        TerminatorKind::Unreachable,
    );
    body.c_strings = source.body.c_strings.clone();
    body.promoted = source.body.promoted.clone();
    for (_, data) in source.body.blocks.iter() {
        let mut statements = Vec::new();
        for statement in &data.statements {
            let mut statement = statement.clone();
            match &mut statement.kind {
                StatementKind::Assign(place, value) => {
                    rebase.place(place, &mut statements);
                    rebase.value(value, &mut statements);
                }
                StatementKind::PlaceMention(place) => rebase.place(place, &mut statements),
                StatementKind::StorageDead(local) => *local = rebase.indices[local.index()],
            }
            statements.push(statement);
        }
        let mut terminator = data.terminator.clone();
        rebase.terminator(&mut terminator.kind, &mut statements);
        body.blocks.alloc(BasicBlockData {
            statements,
            terminator,
        });
    }
    for (&block, &span) in &source.body.fn_spans {
        body.fn_spans
            .insert(BasicBlock::from_raw((block.index() as u32) + 1), span);
    }
    let mut dispatch = Vec::new();
    if dropping {
        let returned = BasicBlock::from_raw(body.blocks.len() as u32 + 1);
        let free = block(
            &mut body,
            vec![],
            TerminatorKind::Call {
                func: lang(&hir.items, "coroutine_free"),
                generic_args: &[],
                args: vec![Operand::Copy(Local::from_raw(1).into())],
                dest: Body::RETURN.into(),
                target: Some(returned),
            },
        );
        block(
            &mut body,
            vec![assign(
                Body::RETURN.into(),
                Rvalue::Use(Operand::Const(Constant::Unit)),
            )],
            TerminatorKind::Return,
        );
        let unstarted = block(
            &mut body,
            vec![],
            TerminatorKind::Drop {
                place: frame_place(pointer, Local::from_raw(1)),
                target: free,
            },
        );
        dispatch.push((0, unstarted));
        for (index, &(_, cancel)) in source.awaits.iter().enumerate() {
            dispatch.push((
                index as u128 + 1,
                BasicBlock::from_raw((cancel.index() as u32) + 1),
            ));
        }
        for data in body
            .blocks
            .values_mut()
            .skip(1)
            .take(source.body.blocks.len())
        {
            if matches!(
                data.terminator.kind,
                TerminatorKind::Return | TerminatorKind::UnwindResume | TerminatorKind::Unreachable
            ) {
                data.terminator.kind = go(free);
            }
        }
        body.blocks[Body::entry()].terminator.kind = TerminatorKind::SwitchInt {
            discr: Operand::Copy(state(pointer)),
            targets: dispatch,
            otherwise: free,
            variants: None,
        };
    } else {
        let TyKind::Adt(poll, _) = signature.ret.kind() else {
            unreachable!();
        };
        let future = hir.items.lang_traits[&Symbol::intern("future_trait")];
        let poll_method = hir.items.traits[future]
            .function(&hir.items, Symbol::intern("poll"))
            .expect("Future::poll");
        let pin = hir.items.lang_enums[&Symbol::intern("pin")];
        dispatch.push((0, BasicBlock::from_raw(1)));
        let old_blocks: Vec<_> = source.body.blocks.iter().map(|(id, _)| id).collect();
        for original in old_blocks {
            let current = BasicBlock::from_raw((original.index() as u32) + 1);
            if matches!(body.blocks[current].terminator.kind, TerminatorKind::Return) {
                body.blocks[current].statements.extend([
                    assign(state(pointer), Rvalue::Use(number(u64::MAX))),
                    assign(
                        Body::RETURN.into(),
                        Rvalue::Aggregate(
                            AggregateKind::Variant(poll.strukt, 0),
                            vec![Operand::Move(frame_place(pointer, Body::RETURN))],
                        ),
                    ),
                ]);
            }
        }
        for (index, &(old, _)) in source.awaits.iter().enumerate() {
            let current = BasicBlock::from_raw((old.index() as u32) + 1);
            let TerminatorKind::Call {
                args,
                generic_args,
                dest,
                target: Some(target),
                ..
            } = body.blocks[current].terminator.kind.clone()
            else {
                unreachable!();
            };
            let future = generic_args[0];
            let reference = Ty::new(TyKind::Ref(Mutability::Mut, future));
            let pin_ty = Ty::adt(pin, hir.items.structs[pin].name, &[reference]);
            let pinned = local(&mut body, pin_ty, LocalKind::Temp);
            let output = body.place_ty(&dest, sigs);
            let poll_ty = Ty::adt(poll.strukt, hir.items.structs[poll.strukt].name, &[output]);
            let result = local(&mut body, poll_ty, LocalKind::Temp);
            let tag = local(&mut body, Ty::int(IntTy::Isize), LocalKind::Temp);
            let poll_block = block(
                &mut body,
                vec![assign(
                    pinned.into(),
                    Rvalue::Aggregate(AggregateKind::Struct(pin), vec![args[0].clone()]),
                )],
                TerminatorKind::Call {
                    func: poll_method,
                    generic_args: Ty::list(&[future]),
                    args: vec![
                        Operand::Move(pinned.into()),
                        Operand::Copy(Local::from_raw(2).into()),
                    ],
                    dest: result.into(),
                    target: None,
                },
            );
            let decide = block(
                &mut body,
                vec![assign(tag.into(), Rvalue::Discriminant(result.into()))],
                TerminatorKind::Unreachable,
            );
            let ready = block(
                &mut body,
                vec![assign(
                    dest,
                    Rvalue::Use(Operand::Move(Place::from(result).downcast(0).field(0))),
                )],
                go(target),
            );
            let pending = block(
                &mut body,
                vec![
                    assign(state(pointer), Rvalue::Use(number(index as u64 + 1))),
                    assign(
                        Body::RETURN.into(),
                        Rvalue::Aggregate(AggregateKind::Variant(poll.strukt, 1), vec![]),
                    ),
                ],
                TerminatorKind::Return,
            );
            if let TerminatorKind::Call { target, .. } =
                &mut body.blocks[poll_block].terminator.kind
            {
                *target = Some(decide);
            }
            body.blocks[decide].terminator.kind = TerminatorKind::SwitchInt {
                discr: Operand::Copy(tag.into()),
                targets: vec![(0, ready)],
                otherwise: pending,
                variants: Some(poll.strukt),
            };
            body.blocks[current].terminator.kind = go(poll_block);
            dispatch.push((index as u128 + 1, poll_block));
        }
        let invalid = block(&mut body, vec![], TerminatorKind::Unreachable);
        body.blocks[Body::entry()].terminator.kind = TerminatorKind::SwitchInt {
            discr: Operand::Copy(state(pointer)),
            targets: dispatch,
            otherwise: invalid,
            variants: None,
        };
    }
    // Drop dispatch enters only cancellation blocks. Clear unreachable code so
    // collection cannot instantiate awaits from the original forward body.
    let mut seen = vec![false; body.blocks.len()];
    let mut work = vec![Body::entry()];
    while let Some(block) = work.pop() {
        if std::mem::replace(&mut seen[block.index()], true) {
            continue;
        }
        work.extend(body.blocks[block].terminator.successors());
    }
    for (index, data) in body.blocks.values_mut().enumerate() {
        if !seen[index] {
            data.statements.clear();
            data.terminator.kind = TerminatorKind::Unreachable;
        }
    }
    body
}

/// Heap-backed execution must still reject source futures whose Rust layout
/// would be recursive. Only storage retained at a suspension creates an edge;
/// owning pointers and references break inline layout dependencies.
pub(crate) fn check_recursive_layouts(
    hir: &Hir,
    sigs: &Signatures,
    closures: &rustc_hash::FxHashMap<(flux_ty::ClosureId, ClosureKind), Body>,
) -> Vec<flux_hir::Diagnostic> {
    use flux_ty::{ClosureId, ClosureOwner};
    use rustc_hash::{FxHashMap, FxHashSet};
    let source_body = |id: ClosureId| match id.owner {
        ClosureOwner::Function(f) => hir.body(f),
        ClosureOwner::Const(c) => hir.const_body(c),
    };
    let mut ids: Vec<_> = closures
        .keys()
        .filter_map(|&(id, kind)| {
            (kind == ClosureKind::FnOnce
                && source_body(id).is_some_and(|(body, _)| {
                    matches!(
                        body.exprs[id.expr],
                        flux_hir::Expr::Closure { is_async: true, .. }
                    )
                }))
            .then_some(id)
        })
        .collect();
    ids.sort_by_key(|id| {
        (
            match id.owner {
                ClosureOwner::Function(f) => (0, f.index()),
                ClosureOwner::Const(c) => (1, c.index()),
            },
            id.expr.index(),
        )
    });
    let indices: FxHashMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut graph = vec![Vec::new(); ids.len()];
    let mut exhausted = vec![false; ids.len()];
    for (index, id) in ids.iter().enumerate() {
        let body = &closures[&(*id, ClosureKind::FnOnce)];
        let awaits: Vec<_> = body
            .blocks
            .iter()
            .filter_map(|(block, data)| {
                matches!(data.terminator.kind, TerminatorKind::Call { func, .. }
                if hir.items.functions[func].intrinsic == Some(Intrinsic::CoroutineAwait))
                .then_some(block)
            })
            .collect();
        let mut types = Vec::new();
        // The closure's captured arguments exist even before its first poll.
        if let Some((_, local)) = body.locals.iter().find(|(_, l)| l.kind == LocalKind::Param)
            && let TyKind::Closure(_, args) = local.ty.kind()
        {
            types.extend(ClosureArgs::split(args).upvar_types().iter().copied());
        }
        if !awaits.is_empty() {
            let live = crate::borrowck::liveness_where(body, &vec![true; body.locals.len()], true);
            for block in awaits {
                types.extend(
                    live[block.index()]
                        .iter()
                        .map(|&local| body.locals[Local::from_raw(local)].ty),
                );
            }
        }
        let mut pending: Vec<_> = types.into_iter().map(|ty| (ty, 0)).collect();
        let mut seen = FxHashSet::default();
        while let Some((ty, depth)) = pending.pop() {
            if depth > 64 {
                exhausted[index] = true;
                continue;
            }
            if !seen.insert(ty) {
                continue;
            }
            let mut push = |ty| pending.push((ty, depth + 1));
            match ty.kind() {
                TyKind::Opaque(opaque) => {
                    let normalized = flux_solve::traits::normalize(&hir.items, sigs, &[], ty);
                    if normalized != ty {
                        push(normalized);
                    } else if let Some(hidden) = sigs.hidden.get(&(opaque.function, opaque.index)) {
                        push(hidden.subst(opaque.args));
                    }
                }
                TyKind::Alias(_) => {
                    let normalized = flux_solve::traits::normalize(&hir.items, sigs, &[], ty);
                    if normalized != ty {
                        push(normalized);
                    }
                }
                TyKind::Adt(adt, args)
                    if flux_solve::is_lang_struct(&hir.items, adt.strukt, "coroutine") =>
                {
                    if let Some(capture) = args.first() {
                        if let TyKind::Closure(closure, _) = capture.kind()
                            && let Some(&target) = indices.get(&closure)
                        {
                            graph[index].push(target);
                        }
                        push(*capture);
                    }
                }
                TyKind::Adt(adt, args)
                    if !flux_solve::is_lang_struct(&hir.items, adt.strukt, "owned_box") =>
                {
                    for &field in flux_solve::field_types(sigs, adt.strukt, args) {
                        push(field);
                    }
                }
                TyKind::Closure(_, args) => {
                    for &capture in ClosureArgs::split(args).upvar_types() {
                        push(capture);
                    }
                }
                TyKind::Tuple(fields) => {
                    for &field in fields {
                        push(field);
                    }
                }
                TyKind::Array(element, _) => push(element),
                _ => {}
            }
        }
        graph[index].sort_unstable();
        graph[index].dedup();
    }
    let mut color = vec![0; ids.len()];
    let mut cyclic = vec![false; ids.len()];
    for root in 0..ids.len() {
        if color[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0)];
        color[root] = 1;
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if *next == graph[node].len() {
                color[node] = 2;
                stack.pop();
                continue;
            }
            let target = graph[node][*next];
            *next += 1;
            if color[target] == 0 {
                color[target] = 1;
                stack.push((target, 0));
            } else if color[target] == 1
                && let Some(start) = stack.iter().position(|&(node, _)| node == target)
            {
                for &(node, _) in &stack[start..] {
                    cyclic[node] = true;
                }
            }
        }
    }
    ids.iter()
        .enumerate()
        .filter_map(|(index, &id)| {
            if !cyclic[index] && !exhausted[index] {
                return None;
            }
            let (_, map) = source_body(id)?;
            let file = match id.owner {
                ClosureOwner::Function(f) => hir.items.functions[f].loc.file,
                ClosureOwner::Const(c) => hir.items.consts[c].loc.file,
            };
            Some(flux_hir::Diagnostic {
                file,
                range: map.exprs.get(id.expr).copied().unwrap_or_default(),
                message: if cyclic[index] {
                    "recursion in an async function requires boxing"
                } else {
                    "async future layout expansion limit reached"
                }
                .into(),
            })
        })
        .collect()
}
