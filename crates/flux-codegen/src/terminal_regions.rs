//! Share identical closed terminal regions within one object.

use std::hash::{Hash, Hasher};

use keel_ir::dominators::ControlFlowGraph;
use keel_ir::fxhash::{FxHashMap, FxHashSet, FxHasher};
use keel_ir::{Block, Function, InstData, Module, Purpose, StackSlot, Value, ValueDef};

const MAX_BLOCKS: usize = 12;
// A shared helper costs a call and another prologue. Look for substantial
// repetition before adding either to the program.
const MIN_REGION_INSTS: usize = 50;
const MIN_GROUP: usize = 4;

struct Candidate {
    at: usize,
    region: Vec<Block>,
}

struct Group {
    helper: Function,
    sites: Vec<Candidate>,
}

pub(super) fn run(module: &mut Module) -> Vec<usize> {
    if module.functions.len() < MIN_GROUP || !module.files.is_empty() || !module.assembly.is_empty()
    {
        return Vec::new();
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_body: FxHashMap<_, Vec<usize>> = FxHashMap::default();
    for (at, function) in module.functions.iter().enumerate() {
        if super::private_returns::eligible(function)
            && let Some((region, helper)) = candidate(function)
        {
            let mut hasher = FxHasher::default();
            helper.fingerprint().hash(&mut hasher);
            for (_, callee) in helper.callees() {
                callee.name.hash(&mut hasher);
                callee.sig.hash(&mut hasher);
            }
            let matching = by_body.entry(hasher.finish()).or_default();
            let site = Candidate { at, region };
            if let Some(&index) = matching
                .iter()
                .find(|&&i| equivalent(&groups[i].helper, &helper))
            {
                groups[index].sites.push(site);
            } else {
                matching.push(groups.len());
                groups.push(Group {
                    helper,
                    sites: vec![site],
                });
            }
        }
    }
    groups.retain(|group| group.sites.len() >= MIN_GROUP);
    if groups.is_empty() {
        return Vec::new();
    }
    assign_names(module, &mut groups);
    let mut seeds = Vec::new();
    for group in groups {
        for site in group.sites {
            rewrite(&mut module.functions[site.at], &site.region, &group.helper);
            seeds.push(site.at);
        }
        seeds.push(module.functions.len());
        module.functions.push(group.helper);
    }
    module.sync_function_attrs();
    seeds
}

fn assign_names(module: &Module, groups: &mut [Group]) {
    let names = module
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .chain(module.decls.iter().map(|d| d.name.as_str()))
        .chain(module.data.iter().map(|d| d.name.as_str()))
        .chain(
            module
                .functions
                .iter()
                .flat_map(|f| f.callees().map(|(_, d)| d.name.as_str())),
        )
        .collect::<FxHashSet<_>>();
    let mut next = 0usize;
    for group in groups {
        loop {
            let name = format!("__flux_terminal_region_{next}");
            next += 1;
            if !names.contains(name.as_str()) {
                group.helper.name = name;
                break;
            }
        }
    }
}

/// Blocks, values and import IDs were assigned in region order by clone_region.
fn equivalent(a: &Function, b: &Function) -> bool {
    a.sig == b.sig
        && a.attrs == b.attrs
        && a.callees().eq(b.callees())
        && a.stack_slots().eq(b.stack_slots())
        && a.blocks().count() == b.blocks().count()
        && a.blocks().zip(b.blocks()).all(|(x, y)| {
            let left = a.block_insts(x);
            let right = b.block_insts(y);
            left.len() == right.len()
                && left.iter().zip(right).all(|(&i, &j)| {
                    a.inst_data(i) == b.inst_data(j)
                        && a.call_contract(i) == b.call_contract(j)
                        && a.inst_facts(i) == b.inst_facts(j)
                })
        })
}

fn candidate(function: &Function) -> Option<(Vec<Block>, Function)> {
    if !function.attrs.nounwind
        || function.attrs.optnone
        || function.attrs.naked
        || function.attrs.interrupt
        || function.attrs.frame_pointer
        || function.attrs.conserve_stack
        || function.attrs.section.is_some()
        || !function.restrict_params.is_empty()
        || function.sig.variadic
        || function.sig.returns_twice
        || !function.sig.returns.is_empty()
        || function.sig.params.len() > 8
        || function
            .sig
            .params
            .iter()
            .any(|p| matches!(p.purpose, Purpose::ByVal(_)))
        || function.debug_loc.is_some()
        || function.debug_return.is_some()
        || !function.debug_vars.is_empty()
        || function.personality.is_some()
        || function.num_insts() < MIN_REGION_INSTS
        || function.blocks().count() > 96
        || function.layout_insts().any(|(_, i)| {
            matches!(
                function.inst_data(i),
                InstData::InlineAsm { .. } | InstData::X86RflagsRegion { .. }
            )
        })
    {
        return None;
    }
    let entry = function.entry_block()?;
    let cfg = ControlFlowGraph::compute(function);
    for block in function.blocks().filter(|&block| block != entry) {
        let insts = function.block_insts(block);
        if insts.len() < 8
            || !insts
                .iter()
                .any(|&inst| matches!(function.inst_data(inst), InstData::Call { .. }))
            || function.inst_data(*insts.last()?).branch_dests().len() < 2
        {
            continue;
        }
        if let Some(region) = region(function, &cfg, entry, block)
            && let Some(helper) = clone_region(function, entry, &region)
        {
            return Some((region, helper));
        }
    }
    None
}

fn region(
    function: &Function,
    cfg: &ControlFlowGraph,
    entry: Block,
    root: Block,
) -> Option<Vec<Block>> {
    let mut order = vec![root];
    let mut set = FxHashSet::from_iter([root]);
    let mut next = 0;
    let mut count = 0;
    let mut slots = FxHashSet::default();
    while next < order.len() {
        let block = order[next];
        next += 1;
        if !function.block_params(block).is_empty() {
            return None;
        }
        let insts = function.block_insts(block);
        count += insts.len();
        for inst in insts {
            if let InstData::StackAddr { slot } = function.inst_data(*inst) {
                slots.insert(*slot);
            }
        }
        for branch in function.inst_data(*insts.last()?).branch_dests() {
            if branch.block == entry || branch.block == root || !branch.args.is_empty() {
                return None;
            }
            if set.insert(branch.block) {
                order.push(branch.block);
                if order.len() > MAX_BLOCKS {
                    return None;
                }
            }
        }
    }
    if order.len() < 5
        || count < MIN_REGION_INSTS
        || order[1..]
            .iter()
            .any(|&block| cfg.preds(block).iter().any(|id| !set.contains(id)))
    {
        return None;
    }
    for (_, inst) in function.layout_insts().filter(|(b, _)| !set.contains(b)) {
        let data = function.inst_data(inst);
        if matches!(data, InstData::StackAddr { slot } if slots.contains(slot)) {
            return None;
        }
        let mut escapes = false;
        data.for_each_operand(|operand| {
            let home = match function.value_def(function.resolve(operand)) {
                ValueDef::Param(block, _) => block,
                ValueDef::Result(inst, _) => function.inst_block(inst),
                ValueDef::Alias(_) => unreachable!("resolved values have a physical definition"),
            };
            escapes |= set.contains(&home);
        });
        if escapes {
            return None;
        }
    }
    Some(order)
}

fn clone_region(original: &Function, entry: Block, region: &[Block]) -> Option<Function> {
    let mut helper = Function::new("__flux_terminal_region_key", original.sig.clone());
    helper.local = true;
    helper.attrs.nounwind = true;
    helper.attrs.noinline = true;
    helper.attrs.fast_math = original.attrs.fast_math;
    helper.attrs.no_sibling_calls = original.attrs.no_sibling_calls;
    helper.attrs.preserve_memory_faults = original.attrs.preserve_memory_faults;
    helper.attrs.target_features = original.attrs.target_features.clone();
    let blocks: FxHashMap<_, _> = region
        .iter()
        .map(|&block| (block, helper.create_block()))
        .collect();
    let mut values: FxHashMap<Value, Value> = original
        .block_params(entry)
        .iter()
        .map(|&value| {
            (
                value,
                helper.append_block_param(blocks[&region[0]], original.value_type(value)),
            )
        })
        .collect();
    let mut slots: FxHashMap<StackSlot, StackSlot> = FxHashMap::default();
    for &old in region {
        let block = blocks[&old];
        for &inst in original.block_insts(old) {
            if original.inst_loc(inst).is_some() {
                return None;
            }
            let mut data = original.inst_data(inst).clone();
            match &mut data {
                InstData::Iconst { .. }
                | InstData::Fconst { .. }
                | InstData::Convert { .. }
                | InstData::Unary { .. }
                | InstData::Binary { .. }
                | InstData::Icmp { .. }
                | InstData::Select { .. }
                | InstData::Load { .. }
                | InstData::Store { .. }
                | InstData::Brif { .. }
                | InstData::Jump { .. }
                | InstData::Switch { .. }
                | InstData::Return { .. }
                | InstData::Trap => {}
                InstData::StackAddr { slot } => {
                    *slot = *slots.entry(*slot).or_insert_with(|| {
                        let old = original.stack_slot(*slot);
                        helper.create_stack_slot(old.size, old.align)
                    });
                }
                InstData::Call { func, .. } => {
                    let decl = original.callee(*func);
                    if decl.sig.returns_twice {
                        return None;
                    }
                    *func = helper.import_function(&decl.name, decl.sig.clone());
                    *helper.callee_mut(*func) = decl.clone();
                }
                _ => return None,
            }
            let mut complete = true;
            data.for_each_operand_mut(|operand| {
                if let Some(&value) = values.get(&original.resolve(*operand)) {
                    *operand = value;
                } else {
                    complete = false;
                }
            });
            if !complete {
                return None;
            }
            for dest in data.branch_dests_mut() {
                dest.block = *blocks.get(&dest.block)?;
            }
            let new = helper.append_inst(block, data);
            if let Some(contract) = original.call_contract(inst) {
                helper.set_call_contract(new, contract.clone()).ok()?;
            }
            if let Some(facts) = original.inst_facts(inst) {
                helper.set_inst_facts(new, *facts).ok()?;
            }
            values.extend(
                original
                    .inst_results(inst)
                    .iter()
                    .copied()
                    .zip(helper.inst_results(new).iter().copied()),
            );
        }
    }
    Some(helper)
}

fn rewrite(function: &mut Function, region: &[Block], helper: &Function) {
    let root = region[0];
    let entry = function
        .entry_block()
        .expect("outlining requires an entry block");
    let args = function.block_params(entry).to_vec();
    let replaced = function.block_insts(root).to_vec();
    function.remove_insts(root, &replaced);
    function.remove_blocks(&region[1..]);
    let callee = function.import_function(&helper.name, helper.sig.clone());
    function.append_inst(
        root,
        InstData::Call {
            func: callee,
            args: args.into(),
        },
    );
    function.append_inst(root, InstData::Return { args: Vec::new() });
}
