//! Builds MIR from a type-checked HIR body.

use flux_arena::{Arena, ArenaMap};
use flux_hir::{
    ArithOp, BinaryOp, Binding, CmpOp, Expr, ExprId, FileHir, FunctionId, Literal, LogicOp, Pat,
    PatId, Res, Stmt, UnaryOp,
};
use flux_syntax::TextRange;
use flux_ty::{InferenceResult, IntTy, Ty, TypedFile};

use crate::{
    AssertMessage, BasicBlock, BasicBlockData, BinOp, Body, Constant, FileMir, Local, LocalDecl,
    LocalKind, Operand, Rvalue, Statement, StatementKind, Terminator, TerminatorKind, UnOp,
};

/// Builds every body of a file and checks initialization. The file must
/// have no diagnostics from lowering or type checking.
pub fn build_file(hir: &FileHir, typed: &TypedFile) -> FileMir {
    let mut bodies = ArenaMap::new();
    let mut diagnostics = Vec::new();
    for (id, _) in hir.bodies.iter() {
        let body = build(hir, typed, id);
        diagnostics.extend(crate::init::check(&body));
        bodies.insert(id, body);
    }
    diagnostics.sort_by_key(|d: &flux_hir::Diagnostic| (d.range.start(), d.range.end()));
    FileMir {
        bodies,
        diagnostics,
    }
}

/// Builds the MIR of `function`. Its body must have lowered and
/// type-checked without errors; anything erroneous becomes unreachable.
pub fn build(hir: &FileHir, typed: &TypedFile, function: FunctionId) -> Body {
    let (body, source_map) = &hir.bodies[function];
    let sig = &typed.sigs[function];
    let mut b = Builder {
        typed,
        body,
        source_map,
        types: &typed.bodies[function],
        locals: Arena::new(),
        blocks: Vec::new(),
        current: None,
        binding_locals: ArenaMap::new(),
        loops: ArenaMap::new(),
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
    for (&pat, &local) in body.params.iter().zip(&params) {
        if let Pat::Bind(binding) = body.pats[pat] {
            b.binding_locals.insert(binding, local);
        }
    }
    let entry = b.new_block();
    b.current = Some(entry);
    b.lower_into(body.root, Body::RETURN);
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
    }
}

#[derive(Clone, Copy)]
struct LoopScope {
    break_block: BasicBlock,
    continue_block: BasicBlock,
    /// Where `break value` stores the value.
    dest: Local,
}

struct Builder<'a> {
    typed: &'a TypedFile,
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
}

impl Builder<'_> {
    fn range(&self, expr: ExprId) -> TextRange {
        self.source_map.exprs.get(expr).copied().unwrap_or_default()
    }

    fn expr_ty(&self, expr: ExprId) -> Ty {
        self.types
            .expr_types
            .get(expr)
            .copied()
            .unwrap_or(Ty::Error)
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

    fn assign(&mut self, dest: Local, rvalue: Rvalue, range: TextRange) {
        let block = self.block();
        self.blocks[block.index()].0.push(Statement {
            kind: StatementKind::Assign(dest, rvalue),
            range,
        });
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
            Expr::Literal(literal) => Operand::Const(self.constant(*literal, expr)),
            Expr::Tuple(fields) if fields.is_empty() => Operand::Const(Constant::Unit),
            _ => {
                let temp = self.temp(self.expr_ty(expr));
                self.lower_into(expr, temp);
                Operand::Copy(temp)
            }
        }
    }

    fn constant(&self, literal: Literal, expr: ExprId) -> Constant {
        match (literal, self.expr_ty(expr)) {
            (Literal::Int { value, .. }, Ty::Int(ty)) => Constant::int(value as i128, ty),
            (Literal::Bool(value), _) => Constant::Bool(value),
            _ => Constant::Unit,
        }
    }

    /// Evaluates `expr` and stores its value in `dest`.
    fn lower_into(&mut self, expr: ExprId, dest: Local) {
        let range = self.range(expr);
        let body = self.body;
        match &body.exprs[expr] {
            Expr::Missing | Expr::Path(Res::Function(_) | Res::Err) => {
                self.terminate(TerminatorKind::Unreachable, range);
            }
            // Only `()` type-checks so far.
            Expr::Tuple(fields) if !fields.is_empty() => {
                self.terminate(TerminatorKind::Unreachable, range);
            }
            Expr::Literal(_) | Expr::Tuple(_) => {
                let operand = self.lower_operand(expr);
                self.assign(dest, Rvalue::Use(operand), range);
            }
            Expr::Path(Res::Local(binding)) => match self.binding_locals.get(*binding) {
                Some(&local) => self.assign(dest, Rvalue::Use(Operand::Copy(local)), range),
                None => self.terminate(TerminatorKind::Unreachable, range),
            },
            Expr::Block { stmts, tail, .. } => {
                for stmt in stmts.iter() {
                    self.lower_stmt(stmt);
                }
                match tail {
                    Some(tail) => self.lower_into(*tail, dest),
                    None => self.assign(dest, Rvalue::Use(Operand::Const(Constant::Unit)), range),
                }
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
                    None => self.assign(dest, Rvalue::Use(Operand::Const(Constant::Unit)), range),
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
                        dest,
                    },
                );
                self.lower_operand(*loop_body);
                self.loops.remove(expr);
                self.goto(head, range);
                self.current = Some(exit);
            }
            Expr::Break { value, target } => {
                let scope = target.and_then(|t| self.loops.get(t).copied());
                let Some(scope) = scope else {
                    self.terminate(TerminatorKind::Unreachable, range);
                    return;
                };
                match value {
                    Some(value) => self.lower_into(*value, scope.dest),
                    None => self.assign(
                        scope.dest,
                        Rvalue::Use(Operand::Const(Constant::Unit)),
                        range,
                    ),
                }
                self.goto(scope.break_block, range);
            }
            Expr::Continue { target } => match target.and_then(|t| self.loops.get(t).copied()) {
                Some(scope) => self.goto(scope.continue_block, range),
                None => self.terminate(TerminatorKind::Unreachable, range),
            },
            Expr::Return { value } => {
                match value {
                    Some(value) => self.lower_into(*value, Body::RETURN),
                    None => self.assign(
                        Body::RETURN,
                        Rvalue::Use(Operand::Const(Constant::Unit)),
                        range,
                    ),
                }
                self.terminate(TerminatorKind::Return, range);
            }
            Expr::Call { callee, args } => {
                let Expr::Path(Res::Function(func)) = body.exprs[*callee] else {
                    self.terminate(TerminatorKind::Unreachable, range);
                    return;
                };
                let args = args.iter().map(|&arg| self.lower_operand(arg)).collect();
                let returns = self.typed.sigs[func].ret != Ty::Never;
                let target = returns.then(|| self.new_block());
                self.terminate(
                    TerminatorKind::Call {
                        func,
                        args,
                        dest,
                        target,
                    },
                    range,
                );
                self.current = target;
            }
            Expr::Unary { op, operand } => {
                let operand = self.lower_operand(*operand);
                let op = match op {
                    UnaryOp::Neg => UnOp::Neg,
                    UnaryOp::Not => UnOp::Not,
                    UnaryOp::Deref => {
                        self.terminate(TerminatorKind::Unreachable, range);
                        return;
                    }
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
                    self.assign(dest, Rvalue::Use(Operand::Const(Constant::Unit)), range);
                }
            },
            Expr::Cast { expr: inner, .. } => {
                let operand = self.lower_operand(*inner);
                let ty = self.expr_ty(expr);
                self.assign(dest, Rvalue::Cast(operand, ty), range);
            }
        }
    }

    fn lower_stmt(&mut self, stmt: &Stmt) {
        match *stmt {
            Stmt::Let { pat, init, .. } => {
                let local = self.bind(pat);
                if let Some(init) = init {
                    match local {
                        Some(local) => self.lower_into(init, local),
                        None => {
                            self.lower_operand(init);
                        }
                    }
                }
            }
            Stmt::Expr { expr, .. } => {
                self.lower_operand(expr);
            }
        }
    }

    /// The local for a `let` pattern's binding, if it has one.
    fn bind(&mut self, pat: PatId) -> Option<Local> {
        let Pat::Bind(binding) = self.body.pats[pat] else {
            return None;
        };
        let data = &self.body.bindings[binding];
        let ty = self
            .types
            .binding_types
            .get(binding)
            .copied()
            .unwrap_or(Ty::Error);
        let local = self.locals.alloc(LocalDecl {
            ty,
            kind: LocalKind::User {
                name: data.name,
                mutable: data.mutable,
            },
        });
        self.binding_locals.insert(binding, local);
        Some(local)
    }

    /// `a && b` is `if a { b } else { false }`; `a || b` is
    /// `if a { true } else { b }`.
    fn lower_logic(
        &mut self,
        op: LogicOp,
        lhs: ExprId,
        rhs: ExprId,
        dest: Local,
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

    fn lower_assign(&mut self, op: Option<ArithOp>, lhs: ExprId, rhs: ExprId, range: TextRange) {
        let Expr::Path(Res::Local(binding)) = self.body.exprs[lhs] else {
            self.terminate(TerminatorKind::Unreachable, range);
            return;
        };
        let Some(&local) = self.binding_locals.get(binding) else {
            self.terminate(TerminatorKind::Unreachable, range);
            return;
        };
        match op {
            None => self.lower_into(rhs, local),
            Some(arith) => {
                // For primitive types the right side is evaluated first.
                let r = self.lower_operand(rhs);
                let ty = self.locals[local].ty;
                let current = self.temp(ty);
                self.assign(current, Rvalue::Use(Operand::Copy(local)), range);
                let rvalue = self.arith(arith, Operand::Copy(current), r, ty, range);
                self.assign(local, rvalue, range);
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
        if let (BinOp::Div | BinOp::Rem, Ty::Int(int)) = (op, ty) {
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
                AssertMessage::DivisionByZero,
                AssertMessage::DivisionOverflow,
            )
        } else {
            (
                AssertMessage::RemainderByZero,
                AssertMessage::RemainderOverflow,
            )
        };
        // A constant divisor needs only the checks it could fail.
        let divisor = match r {
            Operand::Const(Constant::Int { bits, .. }) => Some(*bits),
            _ => None,
        };
        let minus_one = Constant::int(-1, int);
        if divisor.is_none() || divisor == Some(0) {
            let is_zero = self.temp(Ty::Bool);
            let zero = Operand::Const(Constant::int(0, int));
            self.assign(is_zero, Rvalue::BinaryOp(BinOp::Eq, r.clone(), zero), range);
            self.assert(Operand::Copy(is_zero), false, zero_msg, range);
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
            let is_minus_one = self.temp(Ty::Bool);
            self.assign(
                is_minus_one,
                Rvalue::BinaryOp(BinOp::Eq, r.clone(), Operand::Const(minus_one)),
                range,
            );
            let is_min = self.temp(Ty::Bool);
            let min = Operand::Const(Constant::int(min, int));
            self.assign(is_min, Rvalue::BinaryOp(BinOp::Eq, l.clone(), min), range);
            let overflows = self.temp(Ty::Bool);
            self.assign(
                overflows,
                Rvalue::BinaryOp(
                    BinOp::BitAnd,
                    Operand::Copy(is_minus_one),
                    Operand::Copy(is_min),
                ),
                range,
            );
            self.assert(Operand::Copy(overflows), false, overflow_msg, range);
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
