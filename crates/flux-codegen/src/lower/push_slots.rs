//! Initialize vector slots from private arguments without copying unused bytes.

use std::ops::Range;

use flux_mir::{Body, TerminatorKind};
use flux_ty::{Instance, IntTy, Mutability, Ty, TyKind};
use keel_frontend::FunctionBuilder;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::{
    BinaryOp, Block, CallConv, FuncRef, Function, InsertPoint, Inst, InstData, IntCC, Intrinsic,
    MemoryFlags, Module, Purpose, Signature, StackSlot, Type, Value, ValueDef,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Ctx, instance_symbol, keel_type, popped_payload::field};

const MIN_ELEMENT_BYTES: u64 = 128;
// Smaller savings rarely justify an additional helper and capacity check.
const MIN_OMITTED_BYTES: u64 = 48;

#[derive(Clone, Copy)]
pub(super) struct VecLayout {
    bytes: u64,
    align: u64,
    len: u64,
    cap: u64,
    ptr: u64,
}

impl VecLayout {
    fn get(ctx: &mut Ctx<'_>, instance: Instance) -> Option<Self> {
        let sig = ctx.normalized_signature(instance);
        let [receiver, element] = &*sig.params else {
            return None;
        };
        let TyKind::Ref(Mutability::Mut, vector) = receiver.kind() else {
            return None;
        };
        let elt = ctx.layouts.of(*element);
        // A pointer in the call signature can also be the address of an
        // aggregate argument. Only scalar elements can be stored as sent.
        let scalar = cfg!(target_os = "macos")
            && (1..=8).contains(&elt.size)
            && keel_type(*element).is_some();
        const USIZE: Ty = Ty::int(IntTy::Usize);
        if sig.ret != Ty::UNIT
            || !elt.exact
            || !(scalar || (MIN_ELEMENT_BYTES..=u32::MAX.into()).contains(&elt.size))
            || !ctx.layouts.of(vector).exact
            || ctx.layouts.of(vector).align < 8
        {
            return None;
        }
        let (length, len) = field(ctx, vector, "len")?;
        let (buf, buf_offset) = field(ctx, vector, "buf")?;
        let (capacity, cap_offset) = field(ctx, buf, "cap")?;
        let (non_null, ptr_offset) = field(ctx, buf, "ptr")?;
        let (pointer, pointer_offset) = field(ctx, non_null, "pointer")?;
        let cap = buf_offset + cap_offset;
        let ptr = buf_offset + ptr_offset + pointer_offset;
        if length != USIZE
            || capacity != USIZE
            || !matches!(pointer.kind(), TyKind::RawPtr(_, ty) if ty == *element)
            || [len, cap, ptr].iter().any(|offset| offset % 8 != 0)
        {
            return None;
        }
        Some(Self {
            bytes: elt.size,
            align: elt.align,
            len,
            cap,
            ptr,
        })
    }
}

pub(super) fn record_calls(ctx: &mut Ctx<'_>, push: flux_hir::FunctionId, body: &Body) {
    for block in body.blocks.values() {
        let TerminatorKind::Call {
            func, generic_args, ..
        } = &block.terminator.kind
        else {
            continue;
        };
        if *func != push {
            continue;
        }
        let instance = Instance {
            function: *func,
            args: generic_args,
        };
        if ctx.checked_pushes.insert(instance)
            && let Some(layout) = VecLayout::get(ctx, instance)
        {
            let name = instance_symbol(ctx.hir, ctx.symbols, instance);
            ctx.push_layouts.insert(name, layout);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Field {
    offset: u64,
    ty: Type,
    align: u32,
}

struct Site {
    push: Push,
    stores: Vec<(Field, Inst, Value)>,
    prefix: Option<Prefix>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Span {
    bytes: u64,
    align: u32,
}

struct Prefix {
    span: Span,
    source: Value,
    stores: Vec<Inst>,
}

#[derive(Clone, Copy)]
struct Push {
    block: Block,
    call: Inst,
    callee: FuncRef,
    vector: Value,
    layout: VecLayout,
}

struct Argument {
    push: Push,
    stores: Vec<(Field, Inst, Value)>,
    indirect: Vec<Range<u64>>,
    direct: bool,
    escapes: bool,
}

enum Rewrite {
    Fields(Site),
    Ranges(Push, Vec<Range<u64>>),
    Scalar(Push),
}

const MAX_RESULT_CHAIN: usize = 16;
const HELPER_PREFIX: &str = "__flux_push_slot_";

pub(super) fn forward_module(module: &mut Module, layouts: &FxHashMap<String, VecLayout>) {
    let mut extents = ResultExtents {
        producers: module
            .functions
            .iter()
            .filter(|f| {
                (f.local || f.attrs.noninterposable)
                    && matches!(
                        f.sig.params.first().map(|p| &p.purpose),
                        Some(Purpose::Sret(_))
                    )
                    && !f.sig.returns_twice
            })
            .map(|f| (f.name.as_str(), f))
            .collect(),
        bounds: FxHashMap::default(),
        visiting: FxHashSet::default(),
    };

    // Analyze the unmodified module so the write bound never depends on a
    // helper that is still being generated.
    let plans = module
        .functions
        .iter()
        .map(|function| {
            if eligible_function(function) {
                plan_pushes(function, layouts, &mut extents)
            } else {
                Vec::new()
            }
        })
        .collect::<Vec<_>>();
    if plans.iter().all(Vec::is_empty) {
        return;
    }

    let names: FxHashSet<_> = module
        .functions
        .iter()
        .flat_map(|f| {
            std::iter::once(f.name.as_str()).chain(f.callees().map(|(_, d)| d.name.as_str()))
        })
        .chain(module.decls.iter().map(|d| d.name.as_str()))
        .chain(module.data.iter().map(|d| d.name.as_str()))
        .filter(|name| name.starts_with(HELPER_PREFIX))
        .map(str::to_owned)
        .collect();
    let mut field_helpers = FxHashMap::default();
    let mut range_helpers = FxHashMap::default();
    let mut scalar_helpers = FxHashMap::default();
    let mut next_helper = 0;
    let mut bodies = Vec::new();
    for (function, plans) in module.functions.iter_mut().zip(plans) {
        for plan in plans {
            match plan {
                Rewrite::Fields(site) => {
                    let push = site.push;
                    let callee = function.callee(push.callee);
                    let fields: Vec<_> = site.stores.iter().map(|&(field, _, _)| field).collect();
                    let span = site.prefix.as_ref().map(|prefix| prefix.span);
                    let signature = helper_signature(&fields, span);
                    let name = field_helpers
                        .entry((callee.name.clone(), fields.clone(), span))
                        .or_insert_with(|| {
                            let name = next_name(&names, &mut next_helper);
                            bodies.extend(make_helpers(
                                &name,
                                callee,
                                &signature,
                                push.layout,
                                &fields,
                                span,
                            ));
                            name
                        });
                    let mut args = vec![push.vector];
                    if let Some(prefix) = site.prefix {
                        args.push(prefix.source);
                        for store in prefix.stores {
                            function.remove_inst(store);
                        }
                    }
                    for (_, store, value) in site.stores {
                        function.remove_inst(store);
                        args.push(value);
                    }
                    let func = function.import_function(name, signature);
                    function.replace_inst(
                        push.call,
                        InstData::Call {
                            func,
                            args: args.into(),
                        },
                    );
                }
                Rewrite::Ranges(push, written) => {
                    let callee = function.callee(push.callee);
                    let signature = callee.sig.clone();
                    let name = range_helpers
                        .entry((callee.name.clone(), written.clone()))
                        .or_insert_with(|| {
                            let name = next_name(&names, &mut next_helper);
                            bodies.push(make_slot_writer(
                                &name,
                                &signature,
                                push.layout,
                                &callee.name,
                                |b, slot, args| {
                                    for range in &written {
                                        copy_bytes(
                                            b,
                                            args[0],
                                            slot,
                                            range.clone(),
                                            push.layout.align as u32,
                                        );
                                    }
                                },
                            ));
                            name
                        });
                    redirect_push(function, push.call, name, signature);
                }
                Rewrite::Scalar(push) => {
                    let callee = function.callee(push.callee);
                    let mut signature = callee.sig.clone();
                    signature.cc = CallConv::Fast;
                    let name = scalar_helpers
                        .entry(callee.name.clone())
                        .or_insert_with(|| {
                            let name = next_name(&names, &mut next_helper);
                            let slow_name = format!("{name}_slow");
                            bodies.push(make_original_call(&slow_name, &signature, callee));
                            bodies.push(make_slot_writer(
                                &name,
                                &signature,
                                push.layout,
                                &slow_name,
                                |b, slot, args| {
                                    b.ins().store_with_flags(
                                        args[0],
                                        slot,
                                        memory_flags(push.layout.align as u32),
                                    );
                                },
                            ));
                            name
                        });
                    redirect_push(function, push.call, name, signature);
                }
            }
        }
    }
    module.functions.extend(bodies);
    module.sync_function_attrs();
}

fn next_name(names: &FxHashSet<String>, next: &mut usize) -> String {
    loop {
        let name = format!("{HELPER_PREFIX}{next}");
        *next += 2;
        if !names.contains(&name) && !names.contains(&format!("{name}_slow")) {
            return name;
        }
    }
}

fn plan_pushes(
    function: &Function,
    layouts: &FxHashMap<String, VecLayout>,
    extents: &mut ResultExtents,
) -> Vec<Rewrite> {
    let mut slots = FxHashMap::default();
    let mut rewrites = Vec::new();
    for (block, call) in function.layout_insts() {
        let InstData::Call { func, args } = function.inst_data(call) else {
            continue;
        };
        let [vector, element] = args.as_ref() else {
            continue;
        };
        let callee = function.callee(*func);
        let Some(&layout) = layouts.get(&callee.name) else {
            continue;
        };
        if layout.bytes < MIN_ELEMENT_BYTES {
            let [vector_ty, element_ty] = callee.sig.params.as_slice() else {
                continue;
            };
            if vector_ty.ty == Type::Ptr
                && matches!(
                    element_ty.ty,
                    Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Ptr
                )
                && width(element_ty.ty) == Some(layout.bytes)
                && callee
                    .sig
                    .params
                    .iter()
                    .all(|p| p.purpose == Purpose::Normal)
                && callee.sig.returns.is_empty()
                && !callee.sig.variadic
                && !callee.sig.returns_twice
                && callee.attrs.nounwind
                && !args.has_explicit_abi()
                && function.call_contract(call).is_none()
            {
                rewrites.push(Rewrite::Scalar(Push {
                    block,
                    call,
                    callee: *func,
                    vector: *vector,
                    layout,
                }));
            }
            continue;
        }
        if !push_args(function, call, callee, args) {
            continue;
        }
        let Some((slot, 0)) = stack_address(function, *element) else {
            continue;
        };
        let storage = function.stack_slot(slot);
        if u64::from(storage.size) < layout.bytes
            || u64::from(storage.align) < layout.align
            || stack_address(function, *vector).is_some_and(|(receiver, _)| receiver == slot)
        {
            continue;
        }
        let push = Push {
            block,
            call,
            callee: *func,
            vector: *vector,
            layout,
        };
        slots
            .entry(slot)
            .and_modify(|arg: &mut Argument| arg.escapes = true)
            .or_insert(Argument {
                push,
                stores: Vec::new(),
                indirect: Vec::new(),
                direct: u64::from(storage.size) == layout.bytes,
                escapes: false,
            });
    }
    if slots.is_empty() {
        return rewrites;
    }

    // A private argument can only be observed by its push. Build the union of
    // every write, including branch-dependent writes and bounded sret calls.
    for (block, inst) in function.layout_insts() {
        let data = function.inst_data(inst);
        data.for_each_operand(|value| {
            let Some((slot, offset)) = stack_address(function, value) else {
                return;
            };
            let Some(arg) = slots.get_mut(&slot) else {
                return;
            };
            if arg.escapes || inst == arg.push.call || is_offset(function, data, value) {
                return;
            }
            let push = arg.push;
            let max = push.layout.bytes;
            if let InstData::Store {
                addr,
                value: input,
                flags,
            } = *data
                && addr == value
                && input != value
                && !flags.volatile
            {
                let ty = function.value_type(input);
                if width(ty)
                    .and_then(|bytes| offset.checked_add(bytes))
                    .is_some_and(|end| end <= max)
                {
                    let align = flags.align.min(push.layout.align as u32);
                    arg.stores.push((Field { offset, ty, align }, inst, input));
                    arg.direct &= block == push.block
                        && function.inst_index(inst) < function.inst_index(push.call);
                    return;
                }
            }
            if let InstData::Call { func, args } = data
                && let Some((name, _)) = indirect_writer(function, inst, *func, args, value)
                && let Some(end) = extents
                    .get(name, MAX_RESULT_CHAIN)
                    .and_then(|bytes| offset.checked_add(bytes))
                && end <= max
            {
                arg.direct = false;
                arg.indirect.push(offset..end);
                return;
            }
            arg.escapes = true;
        });
    }

    let mut arguments = slots
        .into_values()
        .filter(|arg| !arg.escapes)
        .collect::<Vec<_>>();
    arguments.sort_unstable_by_key(|arg| arg.push.call);
    for mut arg in arguments {
        arg.stores.sort_unstable_by_key(|store| store.0.offset);
        if arg.direct
            && arg.stores.len() >= 2
            && arg
                .stores
                .windows(2)
                .all(|p| field_end(p[0].0) <= p[1].0.offset)
        {
            let mut site = Site {
                push: arg.push,
                stores: arg.stores,
                prefix: None,
            };
            let can_forward = if site.stores.len() <= 7 && site.stores.iter().all(|s| basic(s.0.ty))
            {
                true
            } else if let Some((prefix, count)) = copy_prefix(function, &site, extents) {
                if site.stores.len() - count <= 6
                    && site.stores[count..].iter().all(|s| basic(s.0.ty))
                {
                    site.prefix = Some(prefix);
                    site.stores.drain(..count);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if can_forward {
                rewrites.push(Rewrite::Fields(site));
                continue;
            }
            arg.stores = site.stores;
        }
        arg.indirect.extend(
            arg.stores
                .into_iter()
                .map(|(field, _, _)| field.offset..field_end(field)),
        );
        if let Some(written) = profitable_ranges(arg.indirect, arg.push.layout.bytes) {
            rewrites.push(Rewrite::Ranges(arg.push, written));
        }
    }
    rewrites
}

fn field_end(field: Field) -> u64 {
    field.offset + width(field.ty).expect("argument stores have a known width")
}

fn profitable_ranges(mut ranges: Vec<Range<u64>>, bytes: u64) -> Option<Vec<Range<u64>>> {
    ranges.sort_unstable_by_key(|range| range.start);
    let mut merged: Vec<Range<u64>> = Vec::new();
    for range in ranges {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    let copied: u64 = merged.iter().map(|range| range.end - range.start).sum();
    (!merged.is_empty() && merged.len() <= 4 && bytes - copied >= MIN_OMITTED_BYTES)
        .then_some(merged)
}

fn is_offset(function: &Function, data: &InstData, value: Value) -> bool {
    matches!(data, InstData::Binary { op: BinaryOp::Ptradd, args: [base, delta] }
        if *base == value && constant(function, *delta).is_some())
}

fn indirect_writer<'a>(
    function: &'a Function,
    inst: Inst,
    callee: FuncRef,
    args: &keel_ir::CallArgs,
    address: Value,
) -> Option<(&'a str, u64)> {
    let decl = function.callee(callee);
    let Purpose::Sret(layout) = &decl.sig.params.first()?.purpose else {
        return None;
    };
    let (first, others) = args.split_first()?;
    (*first == address
        && !others.contains(&address)
        && !args.has_explicit_abi()
        && !decl.sig.returns_twice
        && function.call_contract(inst).is_none())
    .then_some((decl.name.as_str(), layout.size()))
}
fn redirect_push(function: &mut Function, call: Inst, name: &str, signature: Signature) {
    let func = function.import_function(name, signature);
    let InstData::Call { args, .. } = function.inst_data_mut(call) else {
        unreachable!("the vector argument plan must refer to its original call");
    };
    let args = std::mem::take(args);
    function.replace_inst(call, InstData::Call { func, args });
}

fn make_original_call(name: &str, signature: &Signature, original: &keel_ir::FuncDecl) -> Function {
    let mut function = Function::new(name, signature.clone());
    function.local = true;
    function.attrs.nounwind = true;
    function.attrs.cold = true;
    function.attrs.noinline = true;
    let mut b = FunctionBuilder::new(&mut function);
    let args = entry(&mut b, signature);
    let callee = b.import_function(&original.name, original.sig.clone());
    b.ins().call(callee, &args);
    b.ins().return_(&[]);
    b.seal_all_blocks();
    b.finalize();
    function
}

fn width(ty: Type) -> Option<u64> {
    match ty {
        Type::Ptr => Some(8),
        Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::F32 | Type::F64 => {
            ty.bytes().map(u64::from)
        }
        Type::Vector(_) => ty.bytes().map(u64::from),
        _ => None,
    }
}

fn basic(ty: Type) -> bool {
    !matches!(ty, Type::Vector(_))
}

/// Recognize a contiguous set of loads copied out of private stack storage.
fn copy_prefix(
    function: &Function,
    site: &Site,
    extents: &mut ResultExtents,
) -> Option<(Prefix, usize)> {
    let mut source = None;
    let mut slot = None;
    let mut stores = Vec::new();
    let mut bytes = 0;
    let mut align = 16.min(site.push.layout.align as u32);
    let mut first_load = function.inst_index(site.push.call);
    let mut loads = FxHashMap::default();
    for &(field, store, data) in &site.stores {
        let ValueDef::Result(load, 0) = function.value_def(function.resolve(data)) else {
            break;
        };
        let InstData::Load { addr, flags, .. } = *function.inst_data(load) else {
            break;
        };
        if flags.volatile || function.inst_block(load) != site.push.block || field.offset != bytes {
            break;
        }
        let Some((origin, delta)) = stack_address(function, addr) else {
            break;
        };
        if delta != bytes || slot.is_some_and(|s| s != origin) {
            break;
        }
        if source.is_none() {
            source = Some(addr);
            slot = Some(origin);
        }
        first_load = first_load.min(function.inst_index(load));
        align = align.min(flags.align);
        bytes += width(field.ty)?;
        stores.push(store);
        loads.insert(function.resolve(data), store);
    }
    if bytes < 64 {
        return None;
    }
    let slot = slot?;
    if bytes > u64::from(function.stack_slot(slot).size) {
        return None;
    }
    let mut snapshots = Vec::new();
    let mut only_reads_into_push = true;
    for (block, inst) in function.layout_insts() {
        let data = function.inst_data(inst);
        let mut escapes = false;
        data.for_each_operand(|value| {
            if let Some(&writer) = loads.get(&function.resolve(value))
                && writer != inst
            {
                only_reads_into_push = false;
            }
            if !stack_address(function, value).is_some_and(|(origin, _)| origin == slot) {
                return;
            }
            let last_store = block != site.push.block || function.inst_index(inst) < first_load;
            match *data {
                InstData::Load { addr, .. } if addr == value => {
                    only_reads_into_push &= loads.contains_key(&function.inst_results(inst)[0]);
                }
                InstData::Store {
                    addr, value: input, ..
                } if addr == value && last_store => {
                    snapshots.push((block, inst, addr, input));
                }
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args: [base, delta],
                } if base == value && constant(function, delta).is_some() => {}
                _ => escapes = true,
            }
        });
        if escapes {
            return None;
        }
    }
    let count = stores.len();
    let mut span = Span { bytes, align };
    let mut source = source?;
    if only_reads_into_push
        && let Some(snapshot) = collapse_snapshot(function, site.push.block, span, snapshots)
    {
        source = snapshot.source;
        stores.extend(snapshot.writers);
        if let Some(bytes) = extents.get(snapshot.producer, MAX_RESULT_CHAIN) {
            span.bytes = span.bytes.min(bytes);
        }
    }
    Some((
        Prefix {
            span,
            source,
            stores,
        },
        count,
    ))
}

struct Snapshot<'a> {
    source: Value,
    writers: Vec<Inst>,
    producer: &'a str,
}

fn collapse_snapshot<'a>(
    function: &'a Function,
    owner: Block,
    span: Span,
    mut writers: Vec<(Block, Inst, Value, Value)>,
) -> Option<Snapshot<'a>> {
    writers.sort_unstable_by_key(|&(_, _, addr, _)| {
        stack_address(function, addr).map(|(_, delta)| delta)
    });
    let mut total = 0;
    let mut source = None;
    let mut origin = None;
    for &(block, _, dst, src) in &writers {
        if block != owner || stack_address(function, dst)?.1 != total {
            return None;
        }
        let ValueDef::Result(load, 0) = function.value_def(function.resolve(src)) else {
            return None;
        };
        let InstData::Load { addr, flags, .. } = *function.inst_data(load) else {
            return None;
        };
        if function.inst_block(load) != owner {
            return None;
        }
        let (slot, bytes) = stack_address(function, addr)?;
        if bytes != total
            || origin.is_some_and(|previous| previous != slot)
            || flags.volatile
            || flags.align < span.align
        {
            return None;
        }
        if source.is_none() {
            source = Some(addr);
            origin = Some(slot);
        }
        total += width(function.value_type(src))?;
    }
    if total != span.bytes {
        return None;
    }
    let origin = origin?;
    if span.bytes > u64::from(function.stack_slot(origin).size) {
        return None;
    }
    let mut producer = None;
    for (block, inst) in function.layout_insts() {
        let data = function.inst_data(inst);
        let mut escapes = false;
        data.for_each_operand(|value| {
            if !stack_address(function, value).is_some_and(|(slot, _)| slot == origin) {
                return;
            }
            match data {
                InstData::Load { addr, .. } if *addr == value => {}
                InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args: [base, delta],
                } if *base == value && constant(function, *delta).is_some() => {}
                InstData::Call { func, args }
                    if block != owner
                        && producer.is_none()
                        && stack_address(function, value) == Some((origin, 0)) =>
                {
                    if let Some((name, bytes)) = indirect_writer(function, inst, *func, args, value)
                        && bytes == span.bytes
                    {
                        producer = Some((block, name));
                    } else {
                        escapes = true;
                    }
                }
                _ => escapes = true,
            }
        });
        if escapes {
            return None;
        }
    }
    let (producer_block, producer) = producer?;
    let cfg = ControlFlowGraph::compute(function);
    let dom = DominatorTree::compute(function, &cfg);
    if !dom.dominates(producer_block, owner) {
        return None;
    }
    Some(Snapshot {
        source: source?,
        writers: writers.into_iter().map(|(_, inst, _, _)| inst).collect(),
        producer,
    })
}

struct ResultExtents<'a> {
    producers: FxHashMap<&'a str, &'a Function>,
    bounds: FxHashMap<&'a str, Option<u64>>,
    visiting: FxHashSet<&'a str>,
}

impl ResultExtents<'_> {
    fn get(&mut self, callee: &str, depth: usize) -> Option<u64> {
        let (&name, &function) = self.producers.get_key_value(callee)?;
        if let Some(&bound) = self.bounds.get(name) {
            return bound;
        }
        if depth == 0 || !self.visiting.insert(name) {
            return None;
        }
        let bound = self.bound(function, depth - 1);
        self.visiting.remove(name);
        self.bounds.insert(name, bound);
        bound
    }

    /// A fresh result slot has no initialized tail unless a callee writes it.
    /// Reject any use that could let the result address escape.
    fn bound(&mut self, function: &Function, depth: usize) -> Option<u64> {
        let Purpose::Sret(layout) = &function.sig.params.first()?.purpose else {
            return None;
        };
        let root = *function.block_params(function.entry_block()?).first()?;
        let mut end = 0;
        for (_, inst) in function.layout_insts() {
            let data = function.inst_data(inst);
            let mut supported = true;
            data.for_each_operand(|value| {
                let Some(offset) = relative_offset(function, value, root) else {
                    return;
                };
                let width = match data {
                    InstData::Binary {
                        op: BinaryOp::Ptradd,
                        args: [base, delta],
                    } if *base == value && constant(function, *delta).is_some() => return,
                    InstData::Store {
                        addr,
                        value: field,
                        flags,
                    } if *addr == value && *field != value && !flags.volatile => {
                        width(function.value_type(*field))
                    }
                    InstData::Intrinsic {
                        id: Intrinsic::Memmove | Intrinsic::Memcpy,
                        args,
                        ..
                    } if args.first() == Some(&value) && args.get(1) != Some(&value) => {
                        args.get(2).and_then(|&v| constant(function, v))
                    }
                    InstData::Call { func, args } | InstData::ReturnCall { func, args } => {
                        indirect_writer(function, inst, *func, args, value)
                            .and_then(|(name, _)| self.get(name, depth))
                    }
                    _ => None,
                };
                if let Some(limit) = width.and_then(|width| offset.checked_add(width)) {
                    end = end.max(limit);
                } else {
                    supported = false;
                }
            });
            if !supported {
                return None;
            }
        }
        (end > 0 && end <= layout.size()).then_some(end)
    }
}

fn constant(function: &Function, value: Value) -> Option<u64> {
    let InstData::Iconst { imm, .. } = definition(function, value)? else {
        return None;
    };
    u64::try_from(*imm).ok()
}
fn relative_offset(function: &Function, mut address: Value, root: Value) -> Option<u64> {
    let mut offset = 0u64;
    while function.resolve(address) != root {
        let InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, delta],
        } = definition(function, address)?
        else {
            return None;
        };
        let InstData::Iconst { imm, .. } = definition(function, *delta)? else {
            return None;
        };
        offset = offset.checked_add(u64::try_from(*imm).ok()?)?;
        address = *base;
    }
    Some(offset)
}

fn definition(function: &Function, value: Value) -> Option<&InstData> {
    let ValueDef::Result(inst, 0) = function.value_def(function.resolve(value)) else {
        return None;
    };
    Some(function.inst_data(inst))
}

fn stack_address(function: &Function, mut address: Value) -> Option<(StackSlot, u64)> {
    let mut offset = 0u64;
    loop {
        match definition(function, address)? {
            InstData::StackAddr { slot } => return Some((*slot, offset)),
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args: [base, delta],
            } => {
                let InstData::Iconst { imm, .. } = definition(function, *delta)? else {
                    return None;
                };
                offset = offset.checked_add(u64::try_from(*imm).ok()?)?;
                address = *base;
            }
            _ => return None,
        }
    }
}

fn eligible_function(function: &Function) -> bool {
    function.attrs.nounwind
        && !function.attrs.optnone
        && function.personality.is_none()
        && function.debug_vars.is_empty()
}

fn push_args(
    function: &Function,
    inst: Inst,
    decl: &keel_ir::FuncDecl,
    args: &keel_ir::CallArgs,
) -> bool {
    args.len() == 2
        && decl.sig.params.len() == 2
        && decl
            .sig
            .params
            .iter()
            .all(|p| p.ty == Type::Ptr && p.purpose == Purpose::Normal)
        && !decl.sig.variadic
        && !decl.sig.returns_twice
        && decl.attrs.nounwind
        && decl.sig.returns.is_empty()
        && !args.has_explicit_abi()
        && function.call_contract(inst).is_none()
}

fn helper_signature(fields: &[Field], span: Option<Span>) -> Signature {
    let types = std::iter::once(Type::Ptr)
        .chain(span.map(|_| Type::Ptr))
        .chain(fields.iter().map(|f| f.ty))
        .collect::<Vec<_>>();
    Signature::new(&types, &[])
}

fn make_helpers(
    name: &str,
    original: &keel_ir::FuncDecl,
    signature: &Signature,
    layout: VecLayout,
    fields: &[Field],
    span: Option<Span>,
) -> [Function; 2] {
    let slow_name = format!("{name}_slow");
    let fast = make_slot_writer(name, signature, layout, &slow_name, |b, slot, args| {
        write_fields(b, slot, args, fields, span);
    });

    let mut slow = Function::new(&slow_name, signature.clone());
    slow.local = true;
    slow.attrs.nounwind = true;
    slow.attrs.cold = true;
    let mut builder = FunctionBuilder::new(&mut slow);
    let args = entry(&mut builder, signature);
    let storage = builder.create_stack_slot(layout.bytes as u32, layout.align as u32);
    let slot = builder.ins().stack_addr(storage);
    write_fields(&mut builder, slot, &args[1..], fields, span);
    let push = builder.import_function(&original.name, original.sig.clone());
    builder.ins().call(push, &[args[0], slot]);
    builder.ins().return_(&[]);
    builder.seal_all_blocks();
    builder.finalize();
    [slow, fast]
}

fn make_slot_writer(
    name: &str,
    signature: &Signature,
    layout: VecLayout,
    full_vector: &str,
    write: impl FnOnce(&mut FunctionBuilder<'_>, Value, &[Value]),
) -> Function {
    let mut function = Function::new(name, signature.clone());
    function.local = true;
    function.attrs.nounwind = true;
    let mut b = FunctionBuilder::new(&mut function);
    let args = entry(&mut b, signature);
    let grow = b.create_block();
    let spare = b.create_block();
    let len_ptr = offset(&mut b, args[0], layout.len);
    let cap_ptr = offset(&mut b, args[0], layout.cap);
    let flags = memory_flags(8);
    let len = b.ins().load_with_flags(Type::I64, len_ptr, flags);
    let cap = b.ins().load_with_flags(Type::I64, cap_ptr, flags);
    let full = b.ins().icmp(IntCC::Eq, len, cap);
    b.ins().brif(full, grow, &[], spare, &[]);

    b.switch_to_block(grow);
    let fallback = b.import_function(full_vector, signature.clone());
    b.function_mut().insert_inst(
        InsertPoint::End(grow),
        InstData::ReturnCall {
            func: fallback,
            args: args.clone().into(),
        },
    );

    b.switch_to_block(spare);
    let buf_ptr = offset(&mut b, args[0], layout.ptr);
    let buf = b.ins().load_with_flags(Type::Ptr, buf_ptr, flags);
    let stride = b.ins().iconst(Type::I64, layout.bytes as i64);
    let index = b.ins().imul(len, stride);
    let slot = b.ins().ptradd(buf, index);
    write(&mut b, slot, &args[1..]);
    let one = b.ins().iconst(Type::I64, 1);
    let new_len = b.ins().iadd(len, one);
    b.ins().store_with_flags(new_len, len_ptr, flags);
    b.ins().return_(&[]);
    b.seal_all_blocks();
    b.finalize();
    function
}

fn entry(builder: &mut FunctionBuilder<'_>, signature: &Signature) -> Vec<Value> {
    let entry = builder.create_block();
    let args = signature
        .params
        .iter()
        .map(|param| builder.append_block_param(entry, param.ty))
        .collect();
    builder.switch_to_block(entry);
    args
}

fn memory_flags(align: u32) -> MemoryFlags {
    MemoryFlags {
        align,
        ..MemoryFlags::default()
    }
}

fn offset(builder: &mut FunctionBuilder<'_>, pointer: Value, bytes: u64) -> Value {
    let delta = builder.ins().iconst(Type::I64, bytes as i64);
    builder.ins().ptradd(pointer, delta)
}

fn write_fields(
    builder: &mut FunctionBuilder<'_>,
    slot: Value,
    args: &[Value],
    fields: &[Field],
    span: Option<Span>,
) {
    let mut values = args;
    if let Some(span) = span {
        copy_bytes(builder, args[0], slot, 0..span.bytes, span.align);
        values = &args[1..];
    }
    for (field, &value) in fields.iter().zip(values) {
        let to = offset(builder, slot, field.offset);
        builder
            .ins()
            .store_with_flags(value, to, memory_flags(field.align));
    }
}

fn copy_bytes(
    builder: &mut FunctionBuilder<'_>,
    source: Value,
    slot: Value,
    range: Range<u64>,
    align: u32,
) {
    let mut cursor = range.start;
    for (width, ty) in [
        (16, Type::Vector(keel_ir::VectorType::I8x16)),
        (8, Type::I64),
        (4, Type::I32),
        (2, Type::I16),
        (1, Type::I8),
    ] {
        while cursor + width <= range.end {
            let flags = memory_flags(
                align
                    .min(width as u32)
                    .min(1 << cursor.trailing_zeros().min(4)),
            );
            let from = offset(builder, source, cursor);
            let to = offset(builder, slot, cursor);
            let value = builder.ins().load_with_flags(ty, from, flags);
            builder.ins().store_with_flags(value, to, flags);
            cursor += width;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_bound_follows_bound_direct_calls_and_rejects_escaping_addresses() {
        let module = keel_ir::parse(
            r#"
decl @unknown(ptr)
func @writes(ptr sret [18 x i64]) cc(c) local {
entry(v0: ptr):
    v1 = iconst.i64 32
    v2 = ptradd v0, v1
    v3 = iconst.i64 17
    store align 8 v3, v2
    return
}
func @wraps(ptr sret [18 x i64]) cc(c) local {
entry(v0: ptr):
    call @writes(v0)
    return
}
func @dynamic(ptr sret [18 x i64], i64) cc(c) local {
entry(v0: ptr, v1: i64):
    v2 = ptradd v0, v1
    v3 = iconst.i64 17
    store v3, v2
    return
}
func @escapes(ptr sret [18 x i64]) cc(c) local {
entry(v0: ptr):
    call @unknown(v0)
    return
}
func @recursive(ptr sret [18 x i64]) cc(c) local {
entry(v0: ptr):
    call @recursive(v0)
    return
}
"#,
        )
        .expect("valid indirect result cases");
        let mut extents = ResultExtents {
            producers: module
                .functions
                .iter()
                .map(|f| (f.name.as_str(), f))
                .collect(),
            bounds: FxHashMap::default(),
            visiting: FxHashSet::default(),
        };
        assert_eq!(extents.get("wraps", 16), Some(40));
        for name in ["dynamic", "escapes", "recursive"] {
            assert_eq!(extents.get(name, 16), None, "{name}");
        }
    }

    #[test]
    fn push_with_observable_stack_slot_keeps_the_original_call() {
        let mut module = keel_ir::parse(
            r#"
decl @push(ptr, ptr) cc(c) nounwind
decl @escape(ptr)
func @caller(ptr, i64, i64) nounwind {
    ss0 = stack_slot 144, align 8
    ss1 = stack_slot 144, align 8
entry(v0: ptr, v1: i64, v2: i64):
    v3 = stack_addr ss0
    v4 = iconst.i64 136
    v5 = ptradd v3, v4
    store align 8 v1, v3
    store align 8 v2, v5
    call @push(v0, v3)
    v6 = stack_addr ss1
    v7 = ptradd v6, v4
    store align 8 v1, v6
    store align 8 v2, v7
    call @escape(v6)
    call @push(v0, v6)
    return
}
"#,
        )
        .expect("valid stack argument cases");
        let layout = VecLayout {
            bytes: 144,
            align: 8,
            len: 16,
            cap: 8,
            ptr: 0,
        };
        forward_module(
            &mut module,
            &FxHashMap::from_iter([("push".to_owned(), layout)]),
        );
        keel_ir::verify_module(&module).expect("valid rewritten module");
        let names: Vec<_> = module.functions[0]
            .layout_insts()
            .filter_map(|(_, inst)| {
                let InstData::Call { func, .. } = module.functions[0].inst_data(inst) else {
                    return None;
                };
                Some(module.functions[0].callee(*func).name.as_str())
            })
            .collect();
        assert_eq!(names, ["__flux_push_slot_0", "escape", "push"]);
        let slow = &module.functions[1];
        assert!(slow.attrs.cold);
        assert!(slow.layout_insts().any(|(_, inst)| {
            matches!(slow.inst_data(inst), InstData::Call { func, .. } if slow.callee(*func).name == "push")
        }));
    }
}
