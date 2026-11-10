//! Exceptional call edges and typed forwarding for Keel's direct-only invoke.
use super::{Ctx, Lower, Values, aligned};
use flux_mir::{Body, TerminatorKind, UnwindAction};
use keel_frontend::FunctionBuilder;
use keel_ir::{
    AbiParam, BlockCall, Clause, FuncDecl, FuncRef, Function, FunctionAttrs, Inst, InstData,
    Module, SigRef, Signature, Type, Value,
};

pub(crate) const PERSONALITY: &str = "__flux_eh_personality";

pub(super) enum Call {
    Direct {
        callee: FuncRef,
        location: Option<Value>,
    },
    Indirect {
        sig: SigRef,
        pointer: Value,
    },
}

pub(super) fn declare_personality(module: &mut Module) {
    if module.signature("_Unwind_Resume").is_none()
        && module
            .functions
            .iter()
            .any(|f| f.callees().any(|(_, c)| c.name == "_Unwind_Resume"))
    {
        module.decls.push(FuncDecl {
            name: "_Unwind_Resume".into(),
            sig: Signature::new(&[Type::Ptr], &[]),
            attrs: FunctionAttrs {
                noreturn: true,
                ..FunctionAttrs::default()
            },
        });
    }
    if module
        .functions
        .iter()
        .any(|f| f.personality.as_deref() == Some(PERSONALITY))
        && module.signature(PERSONALITY).is_none()
    {
        module.decls.push(FuncDecl {
            name: PERSONALITY.into(),
            sig: Signature::new(
                &[Type::I32, Type::I32, Type::I64, Type::Ptr, Type::Ptr],
                &[Type::I32],
            ),
            attrs: FunctionAttrs {
                nounwind: true,
                ..FunctionAttrs::default()
            },
        });
    }
}

/// Process-root calls share a catch-all pad with no borrowed SSA locals.
pub(super) fn entry_call(
    b: &mut FunctionBuilder<'_>,
    callee: FuncRef,
    args: &[Value],
    landing: Option<keel_ir::Block>,
) -> Inst {
    let Some(landing) = landing else {
        return b.ins().call(callee, args);
    };
    let normal = b.create_block();
    let call = b.ins().build(InstData::Invoke {
        func: callee,
        args: keel_ir::CallArgs::from(args.to_vec()),
        dests: Box::new([
            BlockCall {
                block: normal,
                args: vec![],
            },
            BlockCall {
                block: landing,
                args: vec![],
            },
        ]),
    });
    b.switch_to_block(normal);
    call
}

impl Ctx<'_> {
    fn indirect_thunk(&mut self, original: &Signature) -> (String, Signature) {
        let mut signature = original.clone();
        // Preserve the position and purpose of an existing sret parameter.
        signature.params.push(AbiParam::new(Type::Ptr));
        if let Some(name) = self.indirect_thunks.get(original) {
            return (name.clone(), signature);
        }
        assert!(
            !original.variadic,
            "variadic indirect invoke needs a concrete call signature"
        );
        let name = format!("__flux_unwind_indirect_{}", self.indirect_thunks.len());
        let mut function = Function::new(&name, signature.clone());
        function.local = true;
        // Inlining an indirect call into an invoke would need indirect-invoke
        // IR support. Keep this forwarding frame and its unwind table intact.
        function.attrs.noinline = true;
        let mut b = FunctionBuilder::new(&mut function);
        let entry = b.create_block();
        b.switch_to_block(entry);
        let args: Vec<_> = original
            .params
            .iter()
            .map(|p| b.append_block_param(entry, p.ty))
            .collect();
        let pointer = b.append_block_param(entry, Type::Ptr);
        let sig = b.import_signature(original.clone());
        let call = b.ins().call_indirect(sig, pointer, &args);
        let results = b.function().inst_results(call).to_vec();
        b.ins().return_(&results);
        b.seal_all_blocks();
        b.finalize();
        self.generated_functions.push(function);
        self.indirect_thunks.insert(original.clone(), name.clone());
        (name, signature)
    }
}

impl Lower<'_, '_, '_> {
    pub(super) fn catch_unwind(
        &mut self,
        args: &[flux_mir::Operand],
        dest: &flux_mir::Place,
        target: Option<flux_mir::BasicBlock>,
    ) {
        let pointer = self.scalar(&args[0]).expect("panic callback pointer");
        let data = self.scalar(&args[1]).expect("panic callback data");
        if self.ctx.panic_strategy == flux_mir::PanicStrategy::Abort {
            let sig = self.b.import_signature(Signature::new(&[Type::Ptr], &[]));
            self.b.ins().call_indirect(sig, pointer, &[data]);
            let zero = self.b.ins().iconst(Type::I64, 0);
            let null = self
                .b
                .ins()
                .convert(keel_ir::ConvertOp::IntToPtr, Type::Ptr, zero);
            self.write(dest, &[null]);
        } else {
            self.b.function_mut().personality = Some(PERSONALITY.into());
            let (name, signature) = self.ctx.indirect_thunk(&Signature::new(&[Type::Ptr], &[]));
            let callee = self.b.import_function(&name, signature);
            let normal = self.b.create_block();
            let handler = self.b.create_block();
            let done = self.b.create_block();
            let exception = self.b.append_block_param(done, Type::Ptr);
            self.b.ins().build(InstData::Invoke {
                func: callee,
                args: keel_ir::CallArgs::from(vec![data, pointer]),
                dests: Box::new([
                    BlockCall {
                        block: normal,
                        args: vec![],
                    },
                    BlockCall {
                        block: handler,
                        args: vec![],
                    },
                ]),
            });
            self.b.switch_to_block(normal);
            let zero = self.b.ins().iconst(Type::I64, 0);
            let null = self
                .b
                .ins()
                .convert(keel_ir::ConvertOp::IntToPtr, Type::Ptr, zero);
            self.b.ins().jump(done, &[null]);
            self.b.switch_to_block(handler);
            let pad = self.b.ins().build(InstData::LandingPad {
                clauses: vec![Clause::Catch(None)],
            });
            let caught = self.b.function().inst_results(pad)[0];
            self.b.ins().jump(done, &[caught]);
            self.b.switch_to_block(done);
            self.write(dest, &[exception]);
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

    /// Emits a call and leaves the builder in its normal continuation.
    pub(super) fn emit_call(&mut self, call: Call, values: &[Value]) -> Inst {
        let action = self.unwind_action;
        if !matches!(action, UnwindAction::Cleanup(_) | UnwindAction::Terminate) {
            return match call {
                Call::Direct {
                    callee,
                    location: None,
                } => self.b.ins().call(callee, values),
                Call::Direct {
                    callee,
                    location: Some(location),
                } => {
                    let mut arguments = values.to_vec();
                    arguments.push(location);
                    self.b.ins().call(callee, &arguments)
                }
                Call::Indirect { sig, pointer } => self.b.ins().call_indirect(sig, pointer, values),
            };
        }
        let mut arguments: Values = values.iter().copied().collect();
        let callee = match call {
            Call::Direct { callee, location } => {
                arguments.extend(location);
                callee
            }
            Call::Indirect { sig, pointer } => {
                let original = self.b.function().signature(sig).clone();
                let (name, signature) = self.ctx.indirect_thunk(&original);
                arguments.push(pointer);
                self.b.import_function(&name, signature)
            }
        };
        let catch_all = match action {
            UnwindAction::Cleanup(target) => {
                let propagates = self
                    .cleanup_propagates
                    .get_or_insert_with(|| propagating_blocks(self.body));
                !propagates[target.index()]
            }
            UnwindAction::Terminate => true,
            UnwindAction::Unreachable | UnwindAction::Continue => {
                return self.b.ins().call(callee, &arguments);
            }
        };
        let normal = self.b.create_block();
        let landing = self.b.create_block();
        let call = self.b.ins().build(InstData::Invoke {
            func: callee,
            args: keel_ir::CallArgs::from(arguments.to_vec()),
            dests: Box::new([
                BlockCall {
                    block: normal,
                    args: vec![],
                },
                BlockCall {
                    block: landing,
                    args: vec![],
                },
            ]),
        });
        self.b.switch_to_block(landing);
        let pad = self.b.ins().build(InstData::LandingPad {
            clauses: vec![if catch_all {
                Clause::Catch(None)
            } else {
                Clause::Cleanup
            }],
        });
        let exception = self.b.function().inst_results(pad)[0];
        let slot = self
            .exception_slot
            .expect("exceptional body has an exception slot");
        self.b
            .ins()
            .store_with_flags(exception, slot, aligned(Type::Ptr));
        match action {
            UnwindAction::Cleanup(target) => {
                self.b.ins().jump(self.blocks[target.index()], &[]);
            }
            UnwindAction::Terminate => self.abort_unwind(),
            _ => unreachable!("only cleanup and terminate create landing pads"),
        }
        self.b.switch_to_block(normal);
        call
    }

    pub(super) fn resume_unwind(&mut self) {
        let slot = self.exception_slot.expect("resume has an exception slot");
        let exception = self
            .b
            .ins()
            .load_with_flags(Type::Ptr, slot, aligned(Type::Ptr));
        let resume = self
            .b
            .import_function("_Unwind_Resume", Signature::new(&[Type::Ptr], &[]));
        self.b.ins().call(resume, &[exception]);
        self.b.ins().trap();
    }

    pub(super) fn abort_unwind(&mut self) {
        let abort = self.panic_block();
        self.b.ins().jump(abort, &[]);
    }
}

/// Mark blocks that can reach a propagating exit. A terminating cleanup claims
/// the exception during phase one, even when no outer handler exists.
fn propagating_blocks(body: &Body) -> Vec<bool> {
    // Build flat predecessor lists once per function, rather than scanning
    // overlapping lexical cleanup ladders for every exceptional call.
    let mut offsets = vec![0usize; body.blocks.len() + 1];
    let mut propagates = vec![false; body.blocks.len()];
    let mut pending = Vec::new();
    for (block, data) in body.blocks.iter() {
        let term = &data.terminator;
        if matches!(term.kind, TerminatorKind::UnwindResume)
            || term.unwind == UnwindAction::Continue
        {
            propagates[block.index()] = true;
            pending.push(block.index());
        }
        for target in term.successors() {
            offsets[target.index() + 1] += 1;
        }
    }
    for index in 1..offsets.len() {
        offsets[index] += offsets[index - 1];
    }
    let mut predecessors = vec![0usize; offsets[body.blocks.len()]];
    let mut cursor = offsets.clone();
    for (block, data) in body.blocks.iter() {
        for target in data.terminator.successors() {
            let index = target.index();
            predecessors[cursor[index]] = block.index();
            cursor[index] += 1;
        }
    }
    while let Some(block) = pending.pop() {
        for &predecessor in &predecessors[offsets[block]..offsets[block + 1]] {
            if !std::mem::replace(&mut propagates[predecessor], true) {
                pending.push(predecessor);
            }
        }
    }
    propagates
}
