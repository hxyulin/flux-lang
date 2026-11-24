//! Read projected values directly from a consumed vector element.

use std::sync::Arc;

use flux_intern::Symbol;
use flux_mir::layout::Shape;
use flux_mir::{Body, TerminatorKind};
use flux_ty::{Instance, IntTy, Mutability, Ty, TyKind};
use keel_frontend::FunctionBuilder;
use keel_ir::dominators::{ControlFlowGraph, DominatorTree};
use keel_ir::fxhash::{FxHashMap, FxHashSet};
use keel_ir::{
    BinaryOp, Block, Function, InsertPoint, Inst, InstData, IntCC, Intrinsic, MemoryFlags, Purpose,
    Signature, StackSlot, Type, Value, ValueDef,
};

use super::{Ctx, content_symbol, instance_symbol};

const MIN_VALUE_BYTES: u64 = 128;

pub(super) fn forward(
    ctx: &mut Ctx<'_>,
    pop: flux_hir::FunctionId,
    body: &Body,
    function: &mut Function,
) {
    if function.personality.is_some()
        || !function.attrs.nounwind
        || function.attrs.optnone
        || !function.debug_vars.is_empty()
    {
        return;
    }
    let mut candidates = body.blocks.values().filter_map(|block| {
        let TerminatorKind::Call {
            func, generic_args, ..
        } = &block.terminator.kind
        else {
            return None;
        };
        (*func == pop).then_some(Instance {
            function: *func,
            args: generic_args,
        })
    });
    let Some(instance) = candidates.next() else {
        return;
    };
    if candidates.next().is_some() {
        return;
    }
    let Some(info) = PopLayout::get(ctx, instance) else {
        return;
    };
    let name = instance_symbol(ctx.hir, ctx.symbols, instance);
    let mut calls = function.layout_insts().filter_map(|(block, inst)| {
        let InstData::Call { func, args } = function.inst_data(inst) else { return None; };
        let decl = function.callee(*func);
        let [result, vector] = args.as_ref() else { return None; };
        if decl.name != name
            || !matches!(decl.sig.params.first()?.purpose, Purpose::Sret(ref l) if l.size() == info.bytes)
            || args.has_explicit_abi()
            || function.call_contract(inst).is_some()
        {
            return None;
        }
        Some((block, inst, function.resolve(*result), *vector))
    });
    let Some((block, call, result, vector)) = calls.next() else {
        return;
    };
    if calls.next().is_some() {
        return;
    }
    drop(calls);
    let Some(InstData::StackAddr { slot }) = definition(function, result) else {
        return;
    };
    if !can_forward(function, block, call, result, *slot, &info) {
        return;
    }
    let helper_name = if let Some(name) = ctx.pop_slot_helpers.get(&instance) {
        name.clone()
    } else {
        let helper_name = content_symbol("pop_slot", name.as_bytes());
        let empty = flux_mir::eval::Value::Variant {
            strukt: info.option,
            index: info.empty_variant,
            fields: Arc::from([]),
        };
        let empty = ctx.promoted(&empty, info.return_ty);
        ctx.generated_functions
            .push(make_helper(&helper_name, &empty, &info));
        ctx.pop_slot_helpers.insert(instance, helper_name.clone());
        helper_name
    };
    let helper = function.import_function(&helper_name, helper_signature());
    let pointer = function.insert_inst(
        InsertPoint::Before(call),
        InstData::Call {
            func: helper,
            args: vec![vector].into(),
        },
    );
    function.remove_inst(call);
    function.replace_all_uses(result, function.inst_results(pointer)[0]);
}

struct PopLayout {
    len_offset: u64,
    ptr_offset: u64,
    bytes: u64,
    align: u64,
    return_ty: Ty,
    option: flux_hir::StructId,
    empty_variant: u32,
}

impl PopLayout {
    fn get(ctx: &mut Ctx<'_>, instance: Instance) -> Option<Self> {
        let signature = ctx.normalized_signature(instance);
        let [receiver] = &*signature.params else {
            return None;
        };
        let TyKind::Ref(Mutability::Mut, vector) = receiver.kind() else {
            return None;
        };
        let TyKind::Adt(_, args) = vector.kind() else {
            return None;
        };
        let [element] = args else {
            return None;
        };
        let elt = ctx.layouts.of(*element);
        if !elt.exact || elt.size < MIN_VALUE_BYTES || elt.size > i64::MAX as u64 {
            return None;
        }
        let TyKind::Adt(option, _) = signature.ret.kind() else {
            return None;
        };
        let variants = flux_solve::variants(&ctx.typed.sigs, signature.ret)?;
        let (empty_variant, occupied_variant) = match variants.as_slice() {
            [[], [field]] if field == element => (0, 1),
            [[field], []] if field == element => (1, 0),
            _ => return None,
        };
        let ret = ctx.layouts.of(signature.ret);
        let Shape::Enum { tag: Some(tag), .. } = &ret.shape else {
            return None;
        };
        if !matches!(tag.encoding, flux_mir::layout::TagEncoding::Niche { untagged, .. } if untagged == occupied_variant)
            || !ret.exact
            || ret.size != elt.size
            || ret.align != elt.align
            || ret.field_offset(occupied_variant, 0) != 0
        {
            return None;
        }

        let (length, len_offset) = field(ctx, vector, "len")?;
        if length != Ty::int(IntTy::Usize) {
            return None;
        }
        let (buf, buf_offset) = field(ctx, vector, "buf")?;
        let (ptr, ptr_offset) = field(ctx, buf, "ptr")?;
        let (pointer, pointer_offset) = field(ctx, ptr, "pointer")?;
        if !matches!(pointer.kind(), TyKind::RawPtr(_, ty) if ty == *element) {
            return None;
        }
        let ptr_offset = buf_offset + ptr_offset + pointer_offset;
        if ctx.layouts.of(vector).align < 8 || len_offset % 8 != 0 || ptr_offset % 8 != 0 {
            return None;
        }
        Some(Self {
            len_offset,
            ptr_offset,
            bytes: elt.size,
            align: elt.align,
            return_ty: signature.ret,
            option: option.strukt,
            empty_variant,
        })
    }
}

pub(super) fn field(ctx: &Ctx<'_>, ty: Ty, name: &str) -> Option<(Ty, u64)> {
    let TyKind::Adt(adt, _) = ty.kind() else {
        return None;
    };
    let (index, _) = ctx.hir.items.structs[adt.strukt].field(Symbol::intern(name))?;
    let layout = ctx.layouts.of(ty);
    let fields = flux_solve::fields(&ctx.typed.sigs, ty)?;
    Some((*fields.get(index)?, layout.field_offset(0, index as u32)))
}

fn helper_signature() -> Signature {
    Signature::new(&[Type::Ptr], &[Type::Ptr])
}

/// Reproduce the library's len decrement and in-bounds pointer calculation.
/// The empty path supplies the ABI representation of the empty result.
fn make_helper(name: &str, empty_data: &str, layout: &PopLayout) -> Function {
    let mut helper = Function::new(name, helper_signature());
    helper.attrs.nounwind = true;
    helper.local = true;
    let mut builder = FunctionBuilder::new(&mut helper);
    let entry = builder.create_block();
    let empty = builder.create_block();
    let occupied = builder.create_block();
    let vector = builder.append_block_param(entry, Type::Ptr);
    let flags = MemoryFlags {
        align: 8,
        ..MemoryFlags::default()
    };
    builder.switch_to_block(entry);
    let len_offset = builder.ins().iconst(Type::I64, layout.len_offset as i64);
    let length = builder.ins().ptradd(vector, len_offset);
    let len = builder.ins().load_with_flags(Type::I64, length, flags);
    let zero = builder.ins().iconst(Type::I64, 0);
    let is_empty = builder.ins().icmp(IntCC::Eq, len, zero);
    builder.ins().brif(is_empty, empty, &[], occupied, &[]);
    builder.switch_to_block(empty);
    let sentinel = builder.import_data(empty_data);
    let value = builder.ins().symbol_addr(keel_ir::Symbol::Data(sentinel));
    builder.ins().return_(&[value]);
    builder.switch_to_block(occupied);
    let one = builder.ins().iconst(Type::I64, 1);
    let index = builder.ins().isub(len, one);
    builder.ins().store_with_flags(index, length, flags);
    let ptr_field = builder.ins().iconst(Type::I64, layout.ptr_offset as i64);
    let ptr_address = builder.ins().ptradd(vector, ptr_field);
    let buffer = builder.ins().load_with_flags(Type::Ptr, ptr_address, flags);
    let stride = builder.ins().iconst(Type::I64, layout.bytes as i64);
    let offset = builder.ins().imul(index, stride);
    let element = builder.ins().ptradd(buffer, offset);
    builder.ins().return_(&[element]);
    builder.seal_all_blocks();
    builder.finalize();
    helper
}

fn definition(function: &Function, value: Value) -> Option<&InstData> {
    let ValueDef::Result(inst, 0) = function.value_def(function.resolve(value)) else {
        return None;
    };
    Some(function.inst_data(inst))
}

fn constant(function: &Function, value: Value) -> Option<i64> {
    match definition(function, value)? {
        InstData::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}

fn is_stack_pointer(function: &Function, mut pointer: Value) -> bool {
    loop {
        match definition(function, pointer) {
            Some(InstData::StackAddr { .. }) => return true,
            Some(InstData::Binary {
                op: BinaryOp::Ptradd,
                args: [base, _],
            }) => pointer = *base,
            _ => return false,
        }
    }
}

/// The returned pointer is usable only while its elements cannot be changed,
/// and its address must never escape to any code that could observe it.
fn can_forward(
    function: &Function,
    block: Block,
    call: Inst,
    result: Value,
    slot: StackSlot,
    layout: &PopLayout,
) -> bool {
    let cfg = ControlFlowGraph::compute(function);
    let dom = DominatorTree::compute(function, &cfg);
    let mut pointers = FxHashMap::from_iter([(result, 0u64)]);
    let mut derivations = FxHashSet::default();
    loop {
        let count = pointers.len();
        for (_, inst) in function.layout_insts() {
            if let InstData::Binary {
                op: BinaryOp::Ptradd,
                args: [base, offset],
            } = function.inst_data(inst)
                && let Some(&base) = pointers.get(&function.resolve(*base))
                && let Some(offset) = constant(function, *offset)
                && let Some(offset) = base.checked_add_signed(offset)
            {
                pointers.insert(function.inst_results(inst)[0], offset);
                derivations.insert(inst);
            }
        }
        if pointers.len() == count {
            break;
        }
    }
    if function
        .layout_insts()
        .filter(|&(_, inst)| {
            matches!(
                function.inst_data(inst), InstData::StackAddr { slot: other } if *other == slot,
            )
        })
        .count()
        != 1
    {
        return false;
    }
    let offset = |value| pointers.get(&function.resolve(value)).copied();
    let valid_read = |addr, bytes, align: u64| {
        offset(addr).is_some_and(|offset| {
            layout.align >= align
                && offset % align == 0
                && offset
                    .checked_add(bytes)
                    .is_some_and(|end| end <= layout.bytes)
        })
    };
    let mut reads = FxHashSet::default();
    let mut needed = FxHashSet::default();
    for (owner, inst) in function.layout_insts() {
        if inst == call {
            continue;
        }
        let data = function.inst_data(inst);
        let mut uses = 0;
        data.for_each_operand(|value| uses += usize::from(offset(value).is_some()));
        if uses == 0 {
            continue;
        }
        if (owner == block && function.inst_index(inst) <= function.inst_index(call))
            || !dom.dominates(block, owner)
            || uses != 1
        {
            return false;
        }
        match data {
            _ if derivations.contains(&inst) => continue,
            InstData::Load { ty, addr, flags }
                if !flags.volatile
                    && valid_read(
                        *addr,
                        u64::from(ty.bytes().unwrap_or(8)),
                        u64::from(flags.align),
                    ) => {}
            InstData::Intrinsic {
                id: Intrinsic::Memmove | Intrinsic::Memcpy,
                args,
                ..
            } if let [dst, src, size] = args.as_slice()
                && let Some(size) =
                    constant(function, *size).and_then(|size| u64::try_from(size).ok())
                && valid_read(*src, size, 1)
                && (matches!(
                    data,
                    InstData::Intrinsic {
                        id: Intrinsic::Memmove,
                        ..
                    }
                ) || is_stack_pointer(function, *dst)) => {}
            _ => return false,
        }
        reads.insert(inst);
        needed.insert(owner);
    }
    if reads.is_empty() {
        return false;
    }
    let mut pending: Vec<_> = needed.iter().copied().collect();
    while let Some(owner) = pending.pop() {
        if owner != block {
            pending.extend(
                cfg.preds(owner)
                    .iter()
                    .copied()
                    .filter(|&pred| needed.insert(pred)),
            );
        }
    }
    for owner in needed.iter().copied() {
        let mut has_later_read = cfg.succs(owner).iter().any(|next| needed.contains(next));
        for &inst in function.block_insts(owner).iter().rev() {
            if inst == call {
                break;
            }
            if has_later_read && !preserves_source(function, function.inst_data(inst)) {
                return false;
            }
            has_later_read |= reads.contains(&inst);
        }
    }
    true
}

fn preserves_source(function: &Function, data: &InstData) -> bool {
    match data {
        InstData::Iconst { .. }
        | InstData::Fconst { .. }
        | InstData::Convert { .. }
        | InstData::Unary { .. }
        | InstData::Binary { .. }
        | InstData::Icmp { .. }
        | InstData::Fcmp { .. }
        | InstData::Select { .. }
        | InstData::StackAddr { .. }
        | InstData::SymbolAddr { .. }
        | InstData::Brif { .. }
        | InstData::Jump { .. }
        | InstData::Switch { .. } => true,
        InstData::Load { flags, .. } => !flags.volatile,
        InstData::Store { addr, flags, .. } => !flags.volatile && is_stack_pointer(function, *addr),
        InstData::Intrinsic {
            id: Intrinsic::Memcpy | Intrinsic::Memmove,
            args,
            ..
        } => args
            .first()
            .is_some_and(|&addr| is_stack_pointer(function, addr)),
        _ => false,
    }
}
