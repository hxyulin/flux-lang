//! Builds MIR from a type-checked HIR body.

use flux_arena::{Arena, ArenaMap};
use flux_hir::{
    ArithOp, BinaryOp, Binding, CmpOp, ConstId, Expr, ExprId, FunctionId, Hir, ItemTree, Literal,
    LogicOp, Pat, PatId, Res, Stmt, StructId, UnaryOp,
};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_ty::{InferenceResult, IntTy, Mutability, Ty, TyKind, TypedFile};

use crate::{
    AggregateKind, AssertMessage, BasicBlock, BasicBlockData, BinOp, Body, Constant, FileMir,
    Local, LocalDecl, LocalKind, Operand, Place, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind, UnOp, projection_ty,
};

/// Builds every body of a file and checks initialization. The file must
/// have no diagnostics from lowering or type checking.
pub fn build_file(hir: &Hir, typed: &TypedFile) -> FileMir {
    let mut diagnostics = Vec::new();
    let mut check = |body: &Body, file| {
        let found = crate::init::check(body, &hir.items, &typed.sigs)
            .into_iter()
            .chain(crate::borrowck::check(body, &hir.items, &typed.sigs));
        diagnostics.extend(found.map(|d| flux_hir::Diagnostic { file, ..d }));
    };
    // Every constant, and every function of the program; `core`'s
    // functions only if something calls them.
    let mut queue: Vec<FunctionId> = Vec::new();
    let mut consts = ArenaMap::new();
    for (id, _) in hir.const_bodies.iter() {
        let body = build_const(hir, typed, id);
        check(&body, hir.items.consts[id].loc.file);
        queue.extend(body.callees());
        consts.insert(id, body);
    }
    queue.extend(
        hir.bodies
            .iter()
            .map(|(id, _)| id)
            .filter(|&id| hir.is_program(id)),
    );
    let mut bodies = ArenaMap::new();
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() || hir.bodies.get(id).is_none() {
            continue;
        }
        let body = build(hir, typed, id);
        check(&body, hir.items.functions[id].loc.file);
        queue.extend(body.callees());
        bodies.insert(id, body);
    }
    let mut const_values = ArenaMap::new();
    if diagnostics.is_empty() {
        diagnostics.extend(evaluate_consts(hir, &bodies, &consts, &mut const_values));
        // rustc lints associated constants even where they are unused.
        let linted = bodies
            .iter()
            .map(|(id, body)| (hir.items.functions[id].loc.file, body))
            .chain(consts.iter().filter_map(|(id, body)| {
                let data = &hir.items.consts[id];
                data.owner.map(|_| (data.loc.file, body))
            }));
        for (file, body) in linted {
            let found = crate::lint::check(body, &typed.sigs, &const_values);
            diagnostics.extend(
                found
                    .into_iter()
                    .map(|d| flux_hir::Diagnostic { file, ..d }),
            );
        }
    }
    flux_hir::Diagnostic::sort(&mut diagnostics);
    FileMir {
        bodies,
        consts,
        const_values,
        diagnostics,
    }
}

/// Evaluates the constants: free ones always, as rustc does, and
/// associated ones where they are used. Errors are E0080, at the
/// constant's value.
fn evaluate_consts(
    hir: &Hir,
    bodies: &ArenaMap<flux_hir::Function, Body>,
    consts: &ArenaMap<flux_hir::Const, Body>,
    values: &mut ArenaMap<flux_hir::Const, crate::eval::Value>,
) -> Vec<flux_hir::Diagnostic> {
    let names = |id: ConstId| {
        hir.items.consts[id]
            .name
            .map_or("_".to_owned(), |n| n.to_string())
    };
    let mut evaluator = crate::eval::Evaluator::new(bodies, consts, &names);
    let mut used = std::collections::HashSet::new();
    for body in bodies
        .iter()
        .map(|(_, b)| b)
        .chain(consts.iter().map(|(_, b)| b))
    {
        used.extend(body.const_items());
    }
    let mut diagnostics = Vec::new();
    for (id, data) in hir.items.consts.iter() {
        if consts.get(id).is_none() {
            continue;
        }
        let wanted = data.owner.is_none() || used.contains(&id);
        match evaluator.eval_const(id) {
            Ok(value) => {
                values.insert(id, value);
            }
            Err(e) if wanted && !e.nested => {
                let mut d = flux_hir::Diagnostic::new(e.range, e.message);
                d.file = data.loc.file;
                diagnostics.push(d);
            }
            Err(_) => {}
        }
    }
    diagnostics
}

/// Builds the MIR of constant `konst`'s value: a body without
/// parameters returning it.
pub fn build_const(hir: &Hir, typed: &TypedFile, konst: ConstId) -> Body {
    let (body, source_map) = &hir.const_bodies[konst];
    let sig = flux_ty::FnSig {
        params: Box::default(),
        ret: typed.sigs.consts[konst],
    };
    let module = hir.items.consts[konst].loc.module;
    build_body(
        hir,
        typed,
        body,
        source_map,
        &typed.consts[konst],
        &sig,
        module,
    )
}

/// Builds the MIR of `function`. Its body must have lowered and
/// type-checked without errors; anything erroneous becomes unreachable.
pub fn build(hir: &Hir, typed: &TypedFile, function: FunctionId) -> Body {
    let (body, source_map) = &hir.bodies[function];
    let sig = &typed.sigs[function];
    let data = &hir.items.functions[function];
    let mut built = build_body(
        hir,
        typed,
        body,
        source_map,
        &typed.bodies[function],
        sig,
        data.loc.module,
    );
    built.inherits_overflow_checks = data.inherits_overflow_checks;
    built
}

fn build_body(
    hir: &Hir,
    typed: &TypedFile,
    body: &flux_hir::Body,
    source_map: &flux_hir::BodySourceMap,
    types: &InferenceResult,
    sig: &flux_ty::FnSig,
    module: flux_hir::ModuleId,
) -> Body {
    let mut b = Builder {
        typed,
        items: &hir.items,
        body,
        source_map,
        types,
        locals: Arena::new(),
        blocks: Vec::new(),
        current: None,
        binding_locals: ArenaMap::new(),
        loops: ArenaMap::new(),
        scopes: Vec::new(),
        c_strings: Vec::new(),
    };
    b.locals.alloc(LocalDecl {
        ty: sig.ret,
        kind: LocalKind::Return,
    });
    let params: Vec<Local> = sig
        .params
        .iter()
        .map(|&ty| {
            b.locals.alloc(LocalDecl {
                ty,
                kind: LocalKind::Param,
            })
        })
        .collect();
    let entry = b.new_block();
    b.current = Some(entry);
    for (&pat, &local) in body.params.iter().zip(&params) {
        match body.pats[pat] {
            Pat::Bind(binding) => {
                let data = &body.bindings[binding];
                b.locals[local].kind = LocalKind::User {
                    name: data.name,
                    mutable: data.mutable,
                };
                b.binding_locals.insert(binding, local);
            }
            _ => {
                b.declare_bindings(pat);
                let range = b.pat_range(pat);
                b.destructure(pat, &local.into(), range);
            }
        }
    }
    b.lower_into(body.root, &Body::RETURN.into());
    let range = b.range(body.root);
    b.terminate(TerminatorKind::Return, range);

    let blocks = b
        .blocks
        .into_iter()
        .map(|(statements, terminator)| BasicBlockData {
            statements,
            terminator: terminator.unwrap_or(Terminator {
                kind: TerminatorKind::Unreachable,
                range: TextRange::default(),
            }),
        });
    let mut arena = Arena::new();
    for block in blocks {
        arena.alloc(block);
    }
    Body {
        locals: b.locals,
        blocks: arena,
        param_count: params.len(),
        c_strings: b.c_strings,
        in_core: hir.items.is_core(module),
        inherits_overflow_checks: false,
    }
}

#[derive(Clone)]
struct LoopScope {
    break_block: BasicBlock,
    continue_block: BasicBlock,
    /// Where `break value` stores the value.
    dest: Place,
    /// How many scopes were open outside the loop; leaving it ends the
    /// others.
    scope_depth: usize,
}

struct Builder<'a> {
    typed: &'a TypedFile,
    items: &'a ItemTree,
    body: &'a flux_hir::Body,
    source_map: &'a flux_hir::BodySourceMap,
    types: &'a InferenceResult,
    locals: Arena<LocalDecl>,
    /// Blocks under construction; a block is finished once it has a
    /// terminator.
    blocks: Vec<(Vec<Statement>, Option<Terminator>)>,
    /// The block being appended to; `None` right after a terminator, until
    /// code needs a block again (then it starts an unreachable one).
    current: Option<BasicBlock>,
    binding_locals: ArenaMap<Binding, Local>,
    loops: ArenaMap<Expr, LoopScope>,
    /// The bindings of each enclosing block, innermost last; they die
    /// when it ends.
    scopes: Vec<Vec<Local>>,
    c_strings: Vec<Box<[u8]>>,
}

/// Whether `expr` denotes a place: a local, a field of one, or what a
/// pointer points to.
fn is_place(body: &flux_hir::Body, expr: ExprId) -> bool {
    match body.exprs[expr] {
        Expr::Path(Res::Local(_)) => true,
        Expr::Field { expr, .. } => is_place(body, expr),
        Expr::Unary {
            op: UnaryOp::Deref, ..
        } => true,
        _ => false,
    }
}

impl Builder<'_> {
    fn range(&self, expr: ExprId) -> TextRange {
        self.source_map.exprs.get(expr).copied().unwrap_or_default()
    }

    fn pat_range(&self, pat: PatId) -> TextRange {
        self.source_map.pats.get(pat).copied().unwrap_or_default()
    }

    fn expr_ty(&self, expr: ExprId) -> Ty {
        self.types
            .expr_types
            .get(expr)
            .copied()
            .unwrap_or(Ty::ERROR)
    }

    fn new_block(&mut self) -> BasicBlock {
        self.blocks.push((Vec::new(), None));
        BasicBlock::from_raw(self.blocks.len() as u32 - 1)
    }

    /// The block to append to, starting a fresh (unreachable) one after a
    /// terminator.
    fn block(&mut self) -> BasicBlock {
        match self.current {
            Some(block) => block,
            None => {
                let block = self.new_block();
                self.current = Some(block);
                block
            }
        }
    }

    fn temp(&mut self, ty: Ty) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            kind: LocalKind::Temp,
        })
    }

    fn assign(&mut self, dest: &Place, rvalue: Rvalue, range: TextRange) {
        let block = self.block();
        self.blocks[block.index()].0.push(Statement {
            kind: StatementKind::Assign(dest.clone(), rvalue),
            range,
        });
    }

    fn assign_unit(&mut self, dest: &Place, range: TextRange) {
        self.assign(dest, Rvalue::Use(Operand::Const(Constant::Unit)), range);
    }

    /// Reads `place`: copies a `Copy` value, moves any other.
    fn read(&self, place: Place) -> Operand {
        if self.place_ty(&place).is_copy() {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    fn place_ty(&self, place: &Place) -> Ty {
        let local_ty = self.locals[place.local].ty;
        place.projection.iter().fold(local_ty, |ty, &elem| {
            projection_ty(ty, elem, &self.typed.sigs)
        })
    }

    /// Ends the scopes of `locals`, if the code is reachable.
    fn storage_dead(&mut self, locals: &[Local], range: TextRange) {
        if self.current.is_none() {
            return;
        }
        let block = self.block();
        for &local in locals.iter().rev() {
            self.blocks[block.index()].0.push(Statement {
                kind: StatementKind::StorageDead(local),
                range,
            });
        }
    }

    /// Ends the scopes inside a loop that `break` or `continue` leaves.
    fn leave_scopes(&mut self, depth: usize, range: TextRange) {
        let locals: Vec<Local> = self.scopes[depth..]
            .iter()
            .rev()
            .flatten()
            .copied()
            .collect();
        self.storage_dead(&locals, range);
    }

    fn terminate(&mut self, kind: TerminatorKind, range: TextRange) {
        let block = self.block();
        self.blocks[block.index()].1 = Some(Terminator { kind, range });
        self.current = None;
    }

    fn goto(&mut self, target: BasicBlock, range: TextRange) {
        if self.current.is_some() {
            self.terminate(TerminatorKind::Goto(target), range);
        }
    }

    /// Evaluates `expr` to an operand: constants directly, anything else
    /// into a new temporary.
    fn lower_operand(&mut self, expr: ExprId) -> Operand {
        match &self.body.exprs[expr] {
            Expr::Literal(literal) => Operand::Const(self.constant(literal, expr)),
            Expr::Tuple(fields) if fields.is_empty() => Operand::Const(Constant::Unit),
            // A negated literal is one constant, as in rustc: `-128i8`
            // does not negate 128.
            Expr::Unary {
                op: UnaryOp::Neg,
                operand,
            } if matches!(
                self.body.exprs[*operand],
                Expr::Literal(Literal::Int { .. })
            ) =>
            {
                let (Expr::Literal(Literal::Int { value, .. }), Some(ty)) =
                    (&self.body.exprs[*operand], self.expr_ty(expr).as_int())
                else {
                    unreachable!("checked above")
                };
                Operand::Const(Constant::int((*value as i128).wrapping_neg(), ty))
            }
            Expr::Path(Res::Const(id)) if self.types.reborrows.get(expr).is_none() => {
                Operand::Const(Constant::Item {
                    id: *id,
                    ty: self.expr_ty(expr),
                })
            }
            _ => {
                let temp = self.temp(self.expr_ty(expr));
                self.lower_into(expr, &temp.into());
                self.read(temp.into())
            }
        }
    }

    /// The place `expr` denotes: itself if it is a place expression,
    /// otherwise a temporary holding its value.
    fn lower_place(&mut self, expr: ExprId) -> Place {
        match self.body.exprs[expr] {
            Expr::Path(Res::Local(binding)) if self.binding_locals.get(binding).is_some() => {
                self.binding_locals[binding].into()
            }
            Expr::Field { expr: base, .. } => {
                let mut place = self.lower_place(base);
                let derefs = self.types.field_derefs.get(expr).copied().unwrap_or(0);
                for _ in 0..derefs {
                    place = place.deref();
                }
                place.field(self.field_index(expr))
            }
            Expr::Unary {
                op: UnaryOp::Deref,
                operand,
            } => self.lower_place(operand).deref(),
            _ => {
                let temp = self.temp(self.expr_ty(expr));
                self.lower_into(expr, &temp.into());
                temp.into()
            }
        }
    }

    fn field_index(&self, expr: ExprId) -> u32 {
        self.types.field_indices.get(expr).copied().unwrap_or(0)
    }

    /// The index of `strukt`'s field `name`.
    fn struct_field(&self, strukt: StructId, name: Symbol) -> u32 {
        self.items.structs[strukt]
            .field(name)
            .map_or(0, |(i, _)| i as u32)
    }

    fn constant(&mut self, literal: &Literal, expr: ExprId) -> Constant {
        match (literal, self.expr_ty(expr).as_int()) {
            (Literal::Int { value, .. }, Some(ty)) => Constant::int(*value as i128, ty),
            (Literal::Bool(value), _) => Constant::Bool(*value),
            (Literal::CStr(bytes), _) => {
                self.c_strings.push(bytes.clone());
                Constant::CStr(self.c_strings.len() as u32 - 1)
            }
            _ => Constant::Unit,
        }
    }

    /// Evaluates `expr` and stores its value in `dest`, reborrowing it if
    /// type checking said so.
    fn lower_into(&mut self, expr: ExprId, dest: &Place) {
        let Some(&mutability) = self.types.reborrows.get(expr) else {
            return self.lower_value(expr, dest);
        };
        let place = if is_place(self.body, expr) {
            self.lower_place(expr)
        } else {
            let temp = self.temp(self.expr_ty(expr));
            self.lower_value(expr, &temp.into());
            temp.into()
        };
        let rvalue = Rvalue::Ref {
            mutability,
            place: place.deref(),
            two_phase: false,
        };
        self.assign(dest, rvalue, self.range(expr));
    }

    /// Evaluates `expr` and stores its value in `dest`.
    fn lower_value(&mut self, expr: ExprId, dest: &Place) {
        let range = self.range(expr);
        let body = self.body;
        match &body.exprs[expr] {
            Expr::Path(Res::Const(_)) => {
                let operand = self.lower_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Missing | Expr::Path(Res::Function(_) | Res::Err) => {
                self.terminate(TerminatorKind::Unreachable, range);
            }
            Expr::Literal(_) => {
                let operand = self.lower_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Tuple(fields) => {
                let operands = fields.iter().map(|&f| self.lower_operand(f)).collect();
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Tuple, operands),
                    range,
                );
            }
            Expr::Path(Res::Struct(strukt)) => {
                // Only unit structs type-check as values.
                self.assign(
                    dest,
                    Rvalue::Aggregate(AggregateKind::Struct(*strukt), Vec::new()),
                    range,
                );
            }
            Expr::Path(Res::Local(binding)) if self.binding_locals.get(*binding).is_none() => {
                self.terminate(TerminatorKind::Unreachable, range);
            }
            // Reads of places, including fields of values that are not
            // places: `make().x`.
            Expr::Path(Res::Local(_))
            | Expr::Field { .. }
            | Expr::Unary {
                op: UnaryOp::Deref, ..
            } => {
                let place = self.lower_place(expr);
                let operand = self.read(place);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Ref {
                mutable,
                expr: inner,
            } => {
                let place = self.lower_place(*inner);
                let mutability = if *mutable {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                let rvalue = Rvalue::Ref {
                    mutability,
                    place,
                    two_phase: false,
                };
                self.assign(dest, rvalue, range);
            }
            Expr::Record {
                strukt,
                fields,
                spread,
            } => self.lower_record(*strukt, fields, *spread, dest, range),
            Expr::Block { stmts, tail, .. } => {
                self.scopes.push(Vec::new());
                for stmt in stmts.iter() {
                    self.lower_stmt(stmt);
                }
                match tail {
                    Some(tail) => self.lower_into(*tail, dest),
                    None => self.assign_unit(dest, range),
                }
                let scope = self.scopes.pop().unwrap_or_default();
                self.storage_dead(&scope, range);
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_operand(*condition);
                let then_block = self.new_block();
                let else_block = self.new_block();
                let join = self.new_block();
                self.terminate(
                    TerminatorKind::SwitchInt {
                        discr: cond,
                        targets: vec![(0, else_block)],
                        otherwise: then_block,
                    },
                    range,
                );
                self.current = Some(then_block);
                self.lower_into(*then_branch, dest);
                self.goto(join, range);
                self.current = Some(else_block);
                match else_branch {
                    Some(else_branch) => self.lower_into(*else_branch, dest),
                    None => self.assign_unit(dest, range),
                }
                self.goto(join, range);
                self.current = Some(join);
            }
            Expr::Loop {
                body: loop_body, ..
            } => {
                let head = self.new_block();
                let exit = self.new_block();
                self.goto(head, range);
                self.current = Some(head);
                self.loops.insert(
                    expr,
                    LoopScope {
                        break_block: exit,
                        continue_block: head,
                        dest: dest.clone(),
                        scope_depth: self.scopes.len(),
                    },
                );
                self.lower_operand(*loop_body);
                self.loops.remove(expr);
                self.goto(head, range);
                self.current = Some(exit);
            }
            Expr::Break { value, target } => {
                let scope = target.and_then(|t| self.loops.get(t).cloned());
                let Some(scope) = scope else {
                    self.terminate(TerminatorKind::Unreachable, range);
                    return;
                };
                match value {
                    Some(value) => self.lower_into(*value, &scope.dest),
                    None => self.assign_unit(&scope.dest, range),
                }
                self.leave_scopes(scope.scope_depth, range);
                self.goto(scope.break_block, range);
            }
            Expr::Continue { target } => match target.and_then(|t| self.loops.get(t).cloned()) {
                Some(scope) => {
                    self.leave_scopes(scope.scope_depth, range);
                    self.goto(scope.continue_block, range);
                }
                None => self.terminate(TerminatorKind::Unreachable, range),
            },
            Expr::Return { value } => {
                let ret = Body::RETURN.into();
                match value {
                    Some(value) => self.lower_into(*value, &ret),
                    None => self.assign_unit(&ret, range),
                }
                self.terminate(TerminatorKind::Return, range);
            }
            Expr::Call { callee, args } => match body.exprs[*callee] {
                Expr::Path(Res::Function(func)) => {
                    let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                    self.call(func, args, dest, range);
                }
                Expr::Path(Res::Struct(strukt)) => {
                    let operands = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                    self.assign(
                        dest,
                        Rvalue::Aggregate(AggregateKind::Struct(strukt), operands),
                        range,
                    );
                }
                _ => self.terminate(TerminatorKind::Unreachable, range),
            },
            Expr::MethodCall { receiver, args, .. }
                if let Some(&method) = self.types.methods.get(expr) =>
            {
                let receiver_range = self.range(*receiver);
                let mut place = self.lower_place(*receiver);
                for _ in 0..method.derefs {
                    place = place.deref();
                }
                // The receiver is passed through a temporary, so errors
                // about it point at it rather than at the whole call.
                let ty = match method.autoref {
                    Some(m) => Ty::new(TyKind::Ref(m, self.place_ty(&place))),
                    None => self.place_ty(&place),
                };
                let temp = self.temp(ty);
                let rvalue = match method.autoref {
                    Some(mutability) => Rvalue::Ref {
                        mutability,
                        place,
                        two_phase: mutability == Mutability::Mut,
                    },
                    None => Rvalue::Use(self.read(place)),
                };
                self.assign(&temp.into(), rvalue, receiver_range);
                let mut operands = vec![self.read(temp.into())];
                operands.extend(args.iter().map(|&arg| self.lower_operand(arg)));
                self.call(method.function, operands, dest, range);
            }
            Expr::MethodCall { receiver, args, .. } => {
                // Only `CStr::as_ptr` type-checks: the reference is the
                // pointer.
                let receiver = self.lower_operand(*receiver);
                for &arg in args.iter() {
                    self.lower_operand(arg);
                }
                self.assign(dest, Rvalue::Use(receiver), range);
            }
            Expr::Unary {
                op: UnaryOp::Neg,
                operand: inner,
            } if matches!(body.exprs[*inner], Expr::Literal(Literal::Int { .. })) => {
                let operand = self.lower_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Unary { op, operand } => {
                let operand = self.lower_operand(*operand);
                let op = match op {
                    UnaryOp::Neg => UnOp::Neg,
                    _ => UnOp::Not,
                };
                self.assign(dest, Rvalue::UnaryOp(op, operand), range);
            }
            Expr::Binary { op, lhs, rhs } => match op {
                BinaryOp::Logic(logic) => self.lower_logic(*logic, *lhs, *rhs, dest, range),
                BinaryOp::Arith(arith) => {
                    let lhs_ty = self.expr_ty(*lhs);
                    let l = self.lower_operand(*lhs);
                    let r = self.lower_operand(*rhs);
                    let rvalue = self.arith(*arith, l, r, lhs_ty, range);
                    self.assign(dest, rvalue, range);
                }
                BinaryOp::Cmp(cmp) => {
                    let l = self.lower_operand(*lhs);
                    let r = self.lower_operand(*rhs);
                    let op = match cmp {
                        CmpOp::Eq => BinOp::Eq,
                        CmpOp::Ne => BinOp::Ne,
                        CmpOp::Lt => BinOp::Lt,
                        CmpOp::Le => BinOp::Le,
                        CmpOp::Gt => BinOp::Gt,
                        CmpOp::Ge => BinOp::Ge,
                    };
                    self.assign(dest, Rvalue::BinaryOp(op, l, r), range);
                }
                BinaryOp::Assign(arith) => {
                    self.lower_assign(*arith, *lhs, *rhs, range);
                    self.assign_unit(dest, range);
                }
            },
            Expr::Cast { expr: inner, .. } => {
                let operand = self.lower_operand(*inner);
                let ty = self.expr_ty(expr);
                self.assign(dest, Rvalue::Cast(operand, ty), range);
            }
        }
    }

    /// Calls `func`, storing the result in `dest`.
    fn call(&mut self, func: FunctionId, args: Vec<Operand>, dest: &Place, range: TextRange) {
        let returns = self.typed.sigs[func].ret != Ty::NEVER;
        let target = returns.then(|| self.new_block());
        self.terminate(
            TerminatorKind::Call {
                func,
                args,
                dest: dest.clone(),
                target,
            },
            range,
        );
        self.current = target;
    }

    /// `S { a: 1, b, ..base }`: the given fields in source order, then
    /// the rest read from `base`, which is moved out of only for fields
    /// that are not `Copy`.
    fn lower_record(
        &mut self,
        strukt: StructId,
        fields: &[(Symbol, ExprId)],
        spread: Option<ExprId>,
        dest: &Place,
        range: TextRange,
    ) {
        let count = self.items.structs[strukt].fields.len();
        let mut operands: Vec<Option<Operand>> = vec![None; count];
        for &(name, value) in fields {
            let operand = self.lower_operand(value);
            operands[self.struct_field(strukt, name) as usize] = Some(operand);
        }
        if let Some(base) = spread {
            let base = self.lower_place(base);
            for (i, operand) in operands.iter_mut().enumerate() {
                if operand.is_none() {
                    *operand = Some(self.read(base.field(i as u32)));
                }
            }
        }
        let operands = operands
            .into_iter()
            .map(|o| o.unwrap_or(Operand::Const(Constant::Unit)))
            .collect();
        self.assign(
            dest,
            Rvalue::Aggregate(AggregateKind::Struct(strukt), operands),
            range,
        );
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let { pat, init, .. } => {
                self.declare_bindings(pat);
                let Some(init) = init else {
                    return;
                };
                match self.body.pats[pat] {
                    Pat::Bind(binding) => {
                        let local = self.binding_locals[binding];
                        self.lower_into(init, &local.into());
                    }
                    // `let _ = place;` does not read the place.
                    Pat::Wild if is_place(self.body, init) => {
                        let place = self.lower_place(init);
                        let range = self.range(init);
                        let block = self.block();
                        self.blocks[block.index()].0.push(Statement {
                            kind: StatementKind::PlaceMention(place),
                            range,
                        });
                    }
                    _ => {
                        let place = self.lower_place(init);
                        self.destructure(pat, &place, self.range(init));
                    }
                }
            }
            Stmt::Expr { expr, .. } => {
                self.lower_operand(expr);
            }
        }
    }

    /// A local for each binding in `pat`.
    fn declare_bindings(&mut self, pat: PatId) {
        let mut bindings = Vec::new();
        self.body.pat_bindings(pat, &mut bindings);
        for binding in bindings {
            let data = &self.body.bindings[binding];
            let ty = self
                .types
                .binding_types
                .get(binding)
                .copied()
                .unwrap_or(Ty::ERROR);
            let local = self.locals.alloc(LocalDecl {
                ty,
                kind: LocalKind::User {
                    name: data.name,
                    mutable: data.mutable,
                },
            });
            self.binding_locals.insert(binding, local);
            if let Some(scope) = self.scopes.last_mut() {
                scope.push(local);
            }
        }
    }

    /// Binds `pat`'s bindings to the parts of the value in `place` they
    /// match.
    fn destructure(&mut self, pat: PatId, place: &Place, range: TextRange) {
        match &self.body.pats[pat] {
            Pat::Missing | Pat::Wild | Pat::Unit(_) => {}
            Pat::Bind(binding) => {
                let local = self.binding_locals[*binding];
                let operand = self.read(place.clone());
                self.assign(&local.into(), Rvalue::Use(operand), range);
            }
            Pat::Tuple { pats, rest } | Pat::TupleStruct { pats, rest, .. } => {
                let count = self
                    .typed
                    .sigs
                    .fields(self.place_ty(place))
                    .map_or(0, |f| f.len());
                for (i, &field) in pats.iter().enumerate() {
                    let index = match rest {
                        Some(rest) if i >= *rest as usize => count - (pats.len() - i),
                        _ => i,
                    };
                    self.destructure(field, &place.field(index as u32), range);
                }
            }
            Pat::Record { strukt, fields, .. } => {
                for &(name, field) in fields.iter() {
                    let index = self.struct_field(*strukt, name);
                    self.destructure(field, &place.field(index), range);
                }
            }
        }
    }

    /// `a && b` is `if a { b } else { false }`; `a || b` is
    /// `if a { true } else { b }`.
    fn lower_logic(
        &mut self,
        op: LogicOp,
        lhs: ExprId,
        rhs: ExprId,
        dest: &Place,
        range: TextRange,
    ) {
        let l = self.lower_operand(lhs);
        let rhs_block = self.new_block();
        let short_block = self.new_block();
        let join = self.new_block();
        let (on_false, on_true, short_value) = match op {
            LogicOp::And => (short_block, rhs_block, false),
            LogicOp::Or => (rhs_block, short_block, true),
        };
        self.terminate(
            TerminatorKind::SwitchInt {
                discr: l,
                targets: vec![(0, on_false)],
                otherwise: on_true,
            },
            range,
        );
        self.current = Some(rhs_block);
        self.lower_into(rhs, dest);
        self.goto(join, range);
        self.current = Some(short_block);
        self.assign(
            dest,
            Rvalue::Use(Operand::Const(Constant::Bool(short_value))),
            range,
        );
        self.goto(join, range);
        self.current = Some(join);
    }

    /// `place = rhs` and `place op= rhs`. The right side is evaluated
    /// first, and the assignment itself carries the whole expression's
    /// range, where rustc reports assignment errors.
    fn lower_assign(&mut self, op: Option<ArithOp>, lhs: ExprId, rhs: ExprId, range: TextRange) {
        if !is_place(self.body, lhs) {
            self.terminate(TerminatorKind::Unreachable, range);
            return;
        }
        match op {
            None => {
                let value = self.lower_operand(rhs);
                let place = self.lower_place(lhs);
                self.assign(&place, Rvalue::Use(value), range);
            }
            Some(arith) => {
                let r = self.lower_operand(rhs);
                let place = self.lower_place(lhs);
                let ty = self.place_ty(&place);
                let current = self.temp(ty);
                self.assign(
                    &current.into(),
                    Rvalue::Use(Operand::Copy(place.clone())),
                    range,
                );
                let rvalue = self.arith(arith, Operand::Copy(current.into()), r, ty, range);
                self.assign(&place, rvalue, range);
            }
        }
    }

    /// An arithmetic or bitwise operation, with the checks division needs.
    fn arith(&mut self, op: ArithOp, l: Operand, r: Operand, ty: Ty, range: TextRange) -> Rvalue {
        let op = match op {
            ArithOp::Add => BinOp::Add,
            ArithOp::Sub => BinOp::Sub,
            ArithOp::Mul => BinOp::Mul,
            ArithOp::Div => BinOp::Div,
            ArithOp::Rem => BinOp::Rem,
            ArithOp::Shl => BinOp::Shl,
            ArithOp::Shr => BinOp::Shr,
            ArithOp::BitXor => BinOp::BitXor,
            ArithOp::BitOr => BinOp::BitOr,
            ArithOp::BitAnd => BinOp::BitAnd,
        };
        if let (BinOp::Div | BinOp::Rem, Some(int)) = (op, ty.as_int()) {
            self.check_division(op, &l, &r, int, range);
        }
        Rvalue::BinaryOp(op, l, r)
    }

    /// Asserts that `r` is not zero and, for signed types, that `l / r` is
    /// not `MIN / -1`.
    fn check_division(
        &mut self,
        op: BinOp,
        l: &Operand,
        r: &Operand,
        int: IntTy,
        range: TextRange,
    ) {
        let (zero_msg, overflow_msg) = if op == BinOp::Div {
            (
                AssertMessage::DivisionByZero(l.clone()),
                AssertMessage::DivisionOverflow(l.clone(), r.clone()),
            )
        } else {
            (
                AssertMessage::RemainderByZero(l.clone()),
                AssertMessage::RemainderOverflow(l.clone(), r.clone()),
            )
        };
        // A constant divisor needs only the checks it could fail.
        let divisor = match r {
            Operand::Const(Constant::Int { bits, .. }) => Some(*bits),
            _ => None,
        };
        let minus_one = Constant::int(-1, int);
        if divisor.is_none() || divisor == Some(0) {
            let is_zero = self.temp(Ty::BOOL);
            let zero = Operand::Const(Constant::int(0, int));
            self.assign(
                &is_zero.into(),
                Rvalue::BinaryOp(BinOp::Eq, r.clone(), zero),
                range,
            );
            self.assert(Operand::Copy(is_zero.into()), false, zero_msg, range);
        }
        let may_be_minus_one = match (divisor, minus_one) {
            (Some(bits), Constant::Int { bits: m, .. }) => bits == m,
            _ => true,
        };
        if int.is_signed() && may_be_minus_one {
            let min = if int.bits() == 128 {
                i128::MIN
            } else {
                -(1i128 << (int.bits() - 1))
            };
            let is_minus_one = self.temp(Ty::BOOL);
            self.assign(
                &is_minus_one.into(),
                Rvalue::BinaryOp(BinOp::Eq, r.clone(), Operand::Const(minus_one)),
                range,
            );
            let is_min = self.temp(Ty::BOOL);
            let min = Operand::Const(Constant::int(min, int));
            self.assign(
                &is_min.into(),
                Rvalue::BinaryOp(BinOp::Eq, l.clone(), min),
                range,
            );
            let overflows = self.temp(Ty::BOOL);
            self.assign(
                &overflows.into(),
                Rvalue::BinaryOp(
                    BinOp::BitAnd,
                    Operand::Copy(is_minus_one.into()),
                    Operand::Copy(is_min.into()),
                ),
                range,
            );
            self.assert(Operand::Copy(overflows.into()), false, overflow_msg, range);
        }
    }

    fn assert(&mut self, cond: Operand, expected: bool, msg: AssertMessage, range: TextRange) {
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target: next,
            },
            range,
        );
        self.current = Some(next);
    }
}
