//! Drop glue with normal and exceptional continuations.
use super::{Glue, GlueLen, drop_fn, drop_glue_inner};
use crate::{
    BasicBlock, BasicBlockData, BinOp, Body, Constant, Local, LocalDecl, LocalKind, Operand,
    Rvalue, Statement, StatementKind, Terminator, TerminatorKind, UnwindAction,
};
use flux_hir::{FunctionId, ItemTree};
use flux_syntax::TextRange;
use flux_ty::{IntTy, Mutability, Signatures, Ty, TyKind};

pub(super) fn expand_drops(items: &ItemTree, sigs: &Signatures, body: &mut Body) {
    let Some(drop) = drop_fn(items) else {
        return;
    };
    let unit = body.locals.alloc(LocalDecl {
        ty: Ty::UNIT,
        kind: LocalKind::Temp,
    });
    let blocks: Vec<_> = body.blocks.iter().map(|(block, _)| block).collect();
    let mut dynamic_drop = None;
    for block in blocks {
        let term = body.blocks[block].terminator.clone();
        let (place, ty, target) = match term.kind {
            TerminatorKind::Drop { place, target } => {
                let ty = body.place_ty(&place, sigs);
                (place, ty, target)
            }
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target: Some(target),
            } if items.functions[func]
                .intrinsic
                .is_some_and(|intrinsic| intrinsic.name() == "drop_in_place")
                && generic_args.first().is_some_and(|ty| !ty.is_dyn()) =>
            {
                let pointer = body.locals.alloc(LocalDecl {
                    ty: body.operand_ty(&args[0], sigs),
                    kind: LocalKind::Temp,
                });
                body.blocks[block].statements.push(Statement {
                    kind: StatementKind::Assign(pointer.into(), Rvalue::Use(args[0].clone())),
                    range: term.range,
                });
                let returned = body.blocks.alloc(BasicBlockData {
                    statements: vec![Statement {
                        kind: StatementKind::Assign(
                            dest,
                            Rvalue::Use(Operand::Const(Constant::Unit)),
                        ),
                        range: term.range,
                    }],
                    terminator: Terminator {
                        kind: TerminatorKind::Goto(target),
                        unwind: UnwindAction::Unreachable,
                        range: term.range,
                    },
                });
                body.call_regions.remove(block);
                body.fn_spans.remove(&block);
                (
                    crate::Place::from(pointer).deref(),
                    generic_args[0],
                    returned,
                )
            }
            _ => continue,
        };
        if ty.is_dyn() {
            let intrinsic = *dynamic_drop.get_or_insert_with(|| {
                items
                    .functions
                    .iter()
                    .find_map(|(id, function)| {
                        (function.intrinsic == Some(flux_hir::Intrinsic::DropInPlace)).then_some(id)
                    })
                    .expect("embedded drop_in_place intrinsic")
            });
            let pointer = body.locals.alloc(LocalDecl {
                ty: Ty::new(TyKind::RawPtr(Mutability::Mut, ty)),
                kind: LocalKind::Temp,
            });
            body.blocks[block].statements.push(Statement {
                kind: StatementKind::Assign(
                    pointer.into(),
                    Rvalue::Ref {
                        raw: true,
                        mutability: Mutability::Mut,
                        place,
                        two_phase: false,
                    },
                ),
                range: term.range,
            });
            body.blocks[block].terminator.kind = TerminatorKind::Call {
                func: intrinsic,
                generic_args: Ty::list(&[ty]),
                args: vec![Operand::Move(pointer.into())],
                dest: unit.into(),
                target: Some(target),
            };
            body.call_regions.remove(block);
            body.fn_spans.remove(&block);
            continue;
        }
        let mut glue = Vec::new();
        drop_glue_inner::<true>(items, sigs, place, ty, &mut glue);
        let entry = Expand {
            items,
            sigs,
            body,
            drop,
            unit,
            range: term.range,
        }
        .value(&glue, target, term.unwind);
        let terminator = &mut body.blocks[block].terminator;
        terminator.kind = TerminatorKind::Goto(entry);
        terminator.unwind = UnwindAction::Unreachable;
    }
    body.discard_unreachable();
}

struct Expand<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    body: &'a mut Body,
    drop: FunctionId,
    unit: Local,
    range: TextRange,
}

impl Expand<'_> {
    fn local(&mut self, ty: Ty) -> Local {
        self.body.locals.alloc(LocalDecl {
            ty,
            kind: LocalKind::Temp,
        })
    }

    fn assign(&self, local: Local, value: Rvalue) -> Statement {
        Statement {
            kind: StatementKind::Assign(local.into(), value),
            range: self.range,
        }
    }

    fn push(
        &mut self,
        statements: Vec<Statement>,
        kind: TerminatorKind,
        unwind: UnwindAction,
    ) -> BasicBlock {
        self.body.blocks.alloc(BasicBlockData {
            statements,
            terminator: Terminator {
                kind,
                unwind,
                range: self.range,
            },
        })
    }

    fn exit(&mut self, unwind: UnwindAction) -> Option<BasicBlock> {
        match unwind {
            UnwindAction::Cleanup(block) => Some(block),
            UnwindAction::Continue => Some(self.push(
                Vec::new(),
                TerminatorKind::UnwindResume,
                UnwindAction::Unreachable,
            )),
            UnwindAction::Terminate | UnwindAction::Unreachable => None,
        }
    }

    fn value(&mut self, glue: &[Glue], target: BasicBlock, unwind: UnwindAction) -> BasicBlock {
        if glue.is_empty() {
            return target;
        }
        let unwind = if unwind == UnwindAction::Terminate {
            let abort = self.push(
                Vec::new(),
                TerminatorKind::UnwindTerminate,
                UnwindAction::Unreachable,
            );
            UnwindAction::Cleanup(abort)
        } else {
            unwind
        };
        self.glue(glue, target, unwind)
    }

    fn glue(&mut self, glue: &[Glue], target: BasicBlock, unwind: UnwindAction) -> BasicBlock {
        let mut normal = target;
        let mut cleanup = self.exit(unwind);
        for (index, part) in glue.iter().enumerate().rev() {
            let on_panic = cleanup.map(UnwindAction::Cleanup).unwrap_or(unwind);
            normal = self.part(part, normal, on_panic);
            if index != 0
                && let Some(next) = cleanup
            {
                cleanup = Some(self.part(part, next, UnwindAction::Terminate));
            }
        }
        normal
    }

    fn part(&mut self, part: &Glue, target: BasicBlock, unwind: UnwindAction) -> BasicBlock {
        match part {
            Glue::Value(glue) => self.value(glue, target, unwind),
            Glue::Drop(place, ty) => {
                let pointer = self.local(Ty::new(TyKind::Ref(Mutability::Mut, *ty)));
                let borrow = self.assign(
                    pointer,
                    Rvalue::Ref {
                        raw: false,
                        mutability: Mutability::Mut,
                        place: place.clone(),
                        two_phase: false,
                    },
                );
                self.push(
                    vec![borrow],
                    TerminatorKind::Call {
                        func: self.drop,
                        generic_args: Ty::list(&[*ty]),
                        args: vec![Operand::Move(pointer.into())],
                        dest: self.unit.into(),
                        target: Some(target),
                    },
                    unwind,
                )
            }
            Glue::Unaligned(place, ty, _) => {
                let local = self.local(*ty);
                let mut glue = Vec::new();
                drop_glue_inner::<true>(self.items, self.sigs, local.into(), *ty, &mut glue);
                let entry = self.value(&glue, target, unwind);
                let moved = self.assign(local, Rvalue::Use(Operand::Move(place.clone())));
                self.push(
                    vec![moved],
                    TerminatorKind::Goto(entry),
                    UnwindAction::Unreachable,
                )
            }
            Glue::Variants(place, strukt, variants) => {
                let mut targets = Vec::new();
                for (variant, glue) in variants.iter().enumerate() {
                    if !glue.is_empty() {
                        targets.push((variant as u128, self.glue(glue, target, unwind)));
                    }
                }
                let tag = self.local(Ty::int(flux_ty::discriminant_ty(self.items, *strukt)));
                let read = self.assign(tag, Rvalue::DiscriminantForDrop(place.clone()));
                self.push(
                    vec![read],
                    TerminatorKind::SwitchInt {
                        discr: Operand::Copy(tag.into()),
                        targets,
                        otherwise: target,
                        variants: Some(*strukt),
                    },
                    UnwindAction::Unreachable,
                )
            }
            Glue::Elements { index, len, glue } => self.elements(*index, len, glue, target, unwind),
        }
    }

    fn elements(
        &mut self,
        index: Local,
        len: &GlueLen,
        glue: &[Glue],
        target: BasicBlock,
        unwind: UnwindAction,
    ) -> BasicBlock {
        let counter = self.local(Ty::int(IntTy::Usize));
        let within = self.local(Ty::BOOL);
        let int = |n| Operand::Const(Constant::int(n, IntTy::Usize));
        let mut initial = vec![self.assign(counter, Rvalue::Use(int(0)))];
        let bound = match len {
            GlueLen::Constant(n) => int(i128::from(*n)),
            GlueLen::Slice(place) => {
                let length = self.local(Ty::int(IntTy::Usize));
                initial.push(self.assign(
                    length,
                    Rvalue::Len {
                        place: place.clone(),
                        borrows: false,
                    },
                ));
                Operand::Copy(length.into())
            }
        };
        let glue: Vec<Glue> = glue
            .iter()
            .map(|part| part.reindex(index, counter))
            .collect();
        let head = self.push(
            Vec::new(),
            TerminatorKind::Unreachable,
            UnwindAction::Unreachable,
        );
        let step = self.increment(counter, head);
        let on_panic = if let Some(exit) = self.exit(unwind) {
            let cleanup_head = self.push(
                Vec::new(),
                TerminatorKind::Unreachable,
                UnwindAction::Unreachable,
            );
            let cleanup_step = self.increment(counter, cleanup_head);
            let cleanup_element = self.glue(&glue, cleanup_step, UnwindAction::Terminate);
            self.test(
                cleanup_head,
                counter,
                within,
                bound.clone(),
                cleanup_element,
                exit,
            );
            // Finish the current element's remaining fields, then increment the
            // existing counter. Starting a fresh loop would drop it twice.
            UnwindAction::Cleanup(cleanup_step)
        } else {
            unwind
        };
        let element = self.glue(&glue, step, on_panic);
        self.test(head, counter, within, bound, element, target);
        self.push(
            initial,
            TerminatorKind::Goto(head),
            UnwindAction::Unreachable,
        )
    }

    fn increment(&mut self, counter: Local, target: BasicBlock) -> BasicBlock {
        let increment = self.assign(
            counter,
            Rvalue::BinaryOp(
                BinOp::Add,
                Operand::Copy(counter.into()),
                Operand::Const(Constant::int(1, IntTy::Usize)),
            ),
        );
        self.push(
            vec![increment],
            TerminatorKind::Goto(target),
            UnwindAction::Unreachable,
        )
    }

    fn test(
        &mut self,
        head: BasicBlock,
        counter: Local,
        within: Local,
        bound: Operand,
        element: BasicBlock,
        exit: BasicBlock,
    ) {
        self.body.blocks[head].statements = vec![self.assign(
            within,
            Rvalue::BinaryOp(BinOp::Lt, Operand::Copy(counter.into()), bound),
        )];
        self.body.blocks[head].terminator.kind = TerminatorKind::SwitchInt {
            discr: Operand::Copy(within.into()),
            targets: vec![(0, exit)],
            otherwise: element,
            variants: None,
        };
    }
}
