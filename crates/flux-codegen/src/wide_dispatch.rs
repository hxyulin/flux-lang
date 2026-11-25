use rustc_hash::FxHashSet;

use keel_ir::{InstData, Module, Signature, Value};

pub(crate) fn keep_large_dispatch_arms_out_of_line(module: &mut Module) -> FxHashSet<String> {
    let mut callees = FxHashSet::default();
    let mut callers = FxHashSet::default();
    for function in &module.functions {
        if function.num_blocks() < 128
            || !function.attrs.optimize.is_empty()
            || function.attrs.optnone
            || function.attrs.flatten
        {
            continue;
        }
        let wide_switch = function.layout_insts().find_map(|(_, inst)| {
            if let InstData::Switch { cases, dests, .. } = function.inst_data(inst)
                && cases.len() >= 128
            {
                Some((cases, dests))
            } else {
                None
            }
        });
        let Some((cases, dests)) = wide_switch else {
            continue;
        };
        let mut representative: Option<(&Signature, Vec<Value>)> = None;
        let mut arm_callees = Vec::new();
        for branch in &dests[1..] {
            let [call, jump] = *function.block_insts(branch.block) else {
                continue;
            };
            let InstData::Call { func, args } = function.inst_data(call) else {
                continue;
            };
            let InstData::Jump { dest } = function.inst_data(jump) else {
                continue;
            };
            let callee = function.callee(*func);
            if !branch.args.is_empty()
                || !function.block_params(branch.block).is_empty()
                || !dest.args.is_empty()
                || args.has_explicit_abi()
                || callee.attrs.alwaysinline
            {
                continue;
            }
            let (signature, expected_args) = representative.get_or_insert_with(|| {
                (
                    &callee.sig,
                    args.iter().map(|&arg| function.resolve(arg)).collect(),
                )
            });
            if &callee.sig == *signature
                && args
                    .iter()
                    .map(|&arg| function.resolve(arg))
                    .eq(expected_args.iter().copied())
            {
                arm_callees.push(callee.name.as_str());
            }
        }
        if arm_callees.len() >= 128 && arm_callees.len() >= 3 * cases.len() / 4 {
            callees.extend(arm_callees);
            callers.insert(function.name.as_str());
        }
    }
    if callees.is_empty() {
        return FxHashSet::default();
    }
    // Only alter definitions whose direct callers all match the dispatch shape.
    for function in &module.functions {
        if !callers.contains(function.name.as_str()) {
            for (_, inst) in function.layout_insts() {
                if let InstData::Call { func, .. }
                | InstData::ReturnCall { func, .. }
                | InstData::Invoke { func, .. } = function.inst_data(inst)
                {
                    callees.remove(function.callee(*func).name.as_str());
                }
            }
        }
    }
    let callers: FxHashSet<String> = callers.into_iter().map(str::to_owned).collect();
    let keep_out_of_line: FxHashSet<String> = callees.into_iter().map(str::to_owned).collect();
    for function in &mut module.functions {
        if keep_out_of_line.contains(&function.name) && !function.attrs.alwaysinline {
            function.attrs.noinline = true;
        }
    }
    module.sync_function_attrs();
    callers
}

pub(crate) struct Allocator<'a>(pub &'a FxHashSet<String>);

impl keel_codegen::regalloc::RegisterAllocator for Allocator<'_> {
    fn name(&self) -> &'static str {
        "full"
    }

    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_codegen::regalloc::AllocationTier;
        let tier = if self.0.contains(&input.func.name) {
            AllocationTier::Bounded
        } else {
            AllocationTier::Full
        };
        tier.allocate(input)
    }
}
