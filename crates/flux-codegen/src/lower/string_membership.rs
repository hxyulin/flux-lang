//! Lower a proven membership test over a constant slice of strings.

use std::collections::BTreeMap;

use flux_hir::{FunctionId, ItemTree};
use flux_intern::Symbol;
use flux_mir::eval::Value as ConstValue;
use flux_mir::{
    AggregateKind, BasicBlock, Body, Constant, Local, Operand, Place, ProjectionElem, Rvalue,
    StatementKind, TerminatorKind,
};
use flux_ty::{Instance, IntTy, Mutability, Ty, TyKind};
use keel_frontend::FunctionBuilder;
use keel_ir::{Block, BlockCall, ConvertOp, Function, IntCC, MemoryFlags, Signature, Type, Value};

use super::{Ctx, Location, Lower};

const MAX_PROOF_BLOCKS: usize = 8;
const MAX_PROOF_DEPTH: usize = 5;
// Small tables cost less to scan. Bounds on table and key size keep generated
// code and compilation work proportional to the original expression.
const TABLE_SIZE: std::ops::RangeInclusive<usize> = 12..=256;
const MAX_STRING_BYTES: usize = 32;

pub(super) struct LocalActivity {
    pub(super) definitions: Vec<usize>,
    pub(super) uses: Vec<usize>,
}

impl LocalActivity {
    fn single_use(&self, local: Local) -> bool {
        self.definitions[local.index()] == 1 && self.uses[local.index()] == 1
    }
}

pub(super) fn lower(
    lower: &mut Lower<'_, '_, '_>,
    instance: Instance,
    block: BasicBlock,
    args: &[Operand],
    dest: &Place,
    target: BasicBlock,
) -> Option<()> {
    let [receiver, query] = args else {
        return None;
    };
    let activity = lower.scan_activity.as_ref()?;
    let TyKind::Ref(Mutability::Not, str_ref) = lower.body.operand_ty(query, lower.sigs).kind()
    else {
        return None;
    };
    let TyKind::Ref(Mutability::Mut, iterator) = lower.body.operand_ty(receiver, lower.sigs).kind()
    else {
        return None;
    };
    if str_ref.kind() != TyKind::Ref(Mutability::Not, Ty::STR)
        || lower.body.place_ty(dest, lower.sigs) != Ty::BOOL
    {
        return None;
    }
    let table = fresh_iter_constant(lower.ctx, lower.body, activity, block, receiver, str_ref)?;
    let Constant::Item { id, args, .. } = table else {
        return None;
    };
    // Inspect the actual selected callee, even when the source uses an
    // extension trait. A third-party name never implies purity.
    let items = &lower.ctx.hir.items;
    let mut proof = Proof {
        ctx: lower.ctx,
        any: lang_method(items, "iterator", "any")?,
        eq: lang_method(items, "eq", "eq")?,
        call_mut: lang_method(items, "fn_mut", "call_mut")?,
        str_ref,
        iterator,
        scanned: false,
    };
    if proof.evaluate(instance, vec![Atom::argument(0), Atom::argument(1)], 0)? != Atom::Scan {
        return None;
    }

    let words = string_table(lower.constant_item_value(id, args)?)?;
    let signature = Signature::new(&[Type::Ptr, Type::I64], &[Type::I8]);
    let name = format!(
        "__flux_string_membership_{}",
        lower.ctx.generated_functions.len()
    );
    let function = lookup(name.clone(), signature.clone(), words);
    lower.ctx.generated_functions.push(function);
    let address = lower.operand(query)[0];
    let string = lower.load(Location::Memory(address), str_ref);
    let callee = lower.b.import_function(&name, signature);
    let call = lower.b.ins().call(callee, &string);
    let result = lower.b.function().inst_results(call)[0];
    lower.write(dest, &[result]);
    lower.b.ins().jump(lower.blocks[target.index()], &[]);
    Some(())
}

fn lang_method(items: &ItemTree, lang: &str, method: &str) -> Option<FunctionId> {
    let trait_ = *items.lang_traits.get(&Symbol::intern(lang))?;
    let function = items.traits[trait_].function(items, Symbol::intern(method))?;
    (function.index() < items.core.functions as usize).then_some(function)
}

fn place(operand: &Operand) -> Option<&Place> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => Some(place),
        Operand::Const(_) => None,
    }
}

fn plain_local(place: &Place) -> Option<Local> {
    place.projection.is_empty().then_some(place.local)
}

fn fresh_iter_constant(
    ctx: &Ctx<'_>,
    body: &Body,
    activity: &LocalActivity,
    mut block: BasicBlock,
    receiver: &Operand,
    str_ref: Ty,
) -> Option<Constant> {
    let mut borrowed = plain_local(place(receiver)?)?;
    if !activity.single_use(borrowed) {
        return None;
    }
    let mut iterator = None;
    for _ in 0..MAX_PROOF_BLOCKS {
        for statement in body.blocks[block].statements.iter().rev() {
            if let StatementKind::Assign(
                dest,
                Rvalue::Ref {
                    place,
                    raw: false,
                    mutability: Mutability::Mut,
                    ..
                },
            ) = &statement.kind
                && iterator.is_none()
                && plain_local(dest) == Some(borrowed)
            {
                if !activity.single_use(place.local) {
                    return None;
                }
                match &*place.projection {
                    [] => iterator = Some(place.local),
                    [ProjectionElem::Deref] => borrowed = place.local,
                    _ => return None,
                }
            }
        }
        // Every invocation of the search must follow a new iter() call,
        // including when the search appears inside a loop.
        let mut predecessors = body.blocks.iter().flat_map(|(id, data)| {
            data.terminator
                .successors()
                .filter(move |&next| next == block)
                .map(move |_| id)
        });
        let previous = predecessors.next()?;
        if predecessors.next().is_some() || block == Body::entry() {
            return None;
        }
        if let TerminatorKind::Call {
            func,
            generic_args,
            args,
            dest,
            target,
        } = &body.blocks[previous].terminator.kind
            && iterator.is_some()
            && plain_local(dest) == iterator
            && *target == Some(block)
        {
            let items = &ctx.hir.items;
            let [slice] = args.as_slice() else {
                return None;
            };
            if func.index() >= items.core.functions as usize
                || items.functions[*func].name.as_str() != "iter"
                || *generic_args != [str_ref]
                || !matches!(
                    body.operand_ty(slice, &ctx.typed.sigs).kind(),
                    TyKind::Ref(Mutability::Not, ty) if ty.kind() == TyKind::Slice(str_ref)
                )
            {
                return None;
            }
            return constant_origin(body, activity, slice);
        }
        block = previous;
    }
    None
}

fn constant_origin<'a>(
    body: &'a Body,
    activity: &LocalActivity,
    mut operand: &'a Operand,
) -> Option<Constant> {
    let mut reborrowed = None;
    for _ in 0..MAX_PROOF_BLOCKS {
        let local = if let Some(local) = reborrowed.take() {
            local
        } else if let Operand::Const(constant) = operand {
            return Some(*constant);
        } else {
            plain_local(place(operand)?)?
        };
        if !activity.single_use(local) {
            return None;
        }
        let value = body
            .blocks
            .values()
            .flat_map(|block| &block.statements)
            .find_map(|statement| match &statement.kind {
                StatementKind::Assign(dest, value) if plain_local(dest) == Some(local) => {
                    Some(value)
                }
                _ => None,
            })?;
        match value {
            Rvalue::Use(input) | Rvalue::Unsize(input, _) => operand = input,
            Rvalue::Ref {
                place,
                raw: false,
                mutability: Mutability::Not,
                ..
            } if matches!(&*place.projection, [] | [ProjectionElem::Deref]) => {
                reborrowed = Some(place.local);
            }
            _ => return None,
        }
    }
    None
}

/// Values in a proven straight-line wrapper. A reference to an initialized
/// local sees the same value throughout the proof because writes are forbidden.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Atom {
    Argument(u8, u8),
    Ref(Box<Atom>),
    Fields(Vec<Atom>),
    Equality,
    Scan,
}

impl Atom {
    fn argument(index: u8) -> Self {
        Self::Argument(index, 0)
    }

    fn deref(self) -> Option<Self> {
        match self {
            Self::Argument(index, depth) => Some(Self::Argument(index, depth.checked_add(1)?)),
            Self::Ref(inner) => Some(*inner),
            _ => None,
        }
    }

    fn read(locals: &[Option<Self>], place: &Place) -> Option<Self> {
        Self::project(locals, place.local, &place.projection)
    }

    fn project(
        locals: &[Option<Self>],
        local: Local,
        projections: &[ProjectionElem],
    ) -> Option<Self> {
        let mut value = locals[local.index()].clone()?;
        for projection in projections {
            value = match (*projection, value) {
                (ProjectionElem::Deref, value) => value.deref()?,
                (ProjectionElem::Field(field), Self::Fields(values)) => {
                    values.into_iter().nth(field as usize)?
                }
                _ => return None,
            };
        }
        Some(value)
    }

    fn operand(locals: &[Option<Self>], operand: &Operand) -> Option<Self> {
        Self::read(locals, place(operand)?)
    }
}

struct Proof<'c, 'a> {
    ctx: &'c mut Ctx<'a>,
    any: FunctionId,
    eq: FunctionId,
    call_mut: FunctionId,
    str_ref: Ty,
    iterator: Ty,
    scanned: bool,
}

impl Proof<'_, '_> {
    fn evaluate(&mut self, instance: Instance, inputs: Vec<Atom>, depth: usize) -> Option<Atom> {
        if depth == MAX_PROOF_DEPTH {
            return None;
        }
        let resolved = self.ctx.resolve_instance(instance);
        let items = &self.ctx.hir.items;
        let function = &items.functions[resolved.function];
        if function.is_extern || function.link_name.is_some() || function.intrinsic.is_some() {
            return None;
        }
        if instance.function == self.eq
            && resolved.function.index() < items.core.functions as usize
            && matches!(instance.args, [a, b] if *a == self.str_ref && a == b)
        {
            let [left, right]: [Atom; 2] = inputs.try_into().ok()?;
            let pair = (left.deref()?, right.deref()?);
            return matches!(
                pair,
                (Atom::Argument(2, 1), Atom::Argument(1, 1))
                    | (Atom::Argument(1, 1), Atom::Argument(2, 1))
            )
            .then_some(Atom::Equality);
        }
        if instance.function == self.any && resolved.function == self.any {
            let [receiver, closure]: [Atom; 2] = inputs.try_into().ok()?;
            let [iterator, closure_ty] = instance.args else {
                return None;
            };
            if self.scanned || receiver != Atom::argument(0) || *iterator != self.iterator {
                return None;
            }
            if !matches!(closure_ty.kind(), TyKind::Closure(..)) {
                return None;
            }
            self.scanned = true;
            let item_ref = Ty::new(TyKind::Ref(Mutability::Not, self.str_ref));
            let tuple = Ty::new(TyKind::Tuple(Ty::list(&[item_ref])));
            let call = Instance {
                function: self.call_mut,
                args: Ty::list(&[*closure_ty, tuple]),
            };
            let predicate = vec![
                Atom::Ref(Box::new(closure)),
                Atom::Fields(vec![Atom::argument(2)]),
            ];
            return (self.evaluate(call, predicate, depth + 1)? == Atom::Equality)
                .then_some(Atom::Scan);
        }
        let mir = self.ctx.mir;
        let (template, _) = mir.instance_body(items, resolved)?;
        if template.blocks.len() > MAX_PROOF_BLOCKS || template.locals.len() > 24 {
            return None;
        }
        let body = flux_mir::mono::body_of(self.ctx.hir, &self.ctx.typed.sigs, mir, resolved);
        if body.param_count != inputs.len() {
            return None;
        }
        let mut locals = vec![None; body.locals.len()];
        for (param, input) in body.params().zip(inputs) {
            locals[param.index()] = Some(input);
        }
        let mut block = Body::entry();
        for _ in 0..body.blocks.len() {
            let data = &body.blocks[block];
            for statement in &data.statements {
                let (dest, value) = match &statement.kind {
                    StatementKind::Assign(dest, value) => (dest, value),
                    StatementKind::StorageDead(_) => continue,
                    StatementKind::PlaceMention(_) => return None,
                };
                let dest = plain_local(dest)?;
                if locals[dest.index()].is_some() {
                    return None;
                }
                let value = match value {
                    Rvalue::Use(operand) => Atom::operand(&locals, operand)?,
                    Rvalue::Ref {
                        place, raw: false, ..
                    } => match place.projection.split_last() {
                        Some((ProjectionElem::Deref, prefix)) => {
                            Atom::project(&locals, place.local, prefix)?
                        }
                        _ => Atom::Ref(Box::new(Atom::read(&locals, place)?)),
                    },
                    Rvalue::Aggregate(AggregateKind::Tuple | AggregateKind::Closure, operands) => {
                        Atom::Fields(
                            operands
                                .iter()
                                .map(|arg| Atom::operand(&locals, arg))
                                .collect::<Option<_>>()?,
                        )
                    }
                    _ => return None,
                };
                locals[dest.index()] = Some(value);
            }
            block = match &data.terminator.kind {
                TerminatorKind::Goto(target) => *target,
                TerminatorKind::Return => return locals[Body::RETURN.index()].take(),
                TerminatorKind::Call {
                    func,
                    generic_args,
                    args,
                    dest,
                    target: Some(target),
                } => {
                    let dest = plain_local(dest)?;
                    if locals[dest.index()].is_some() {
                        return None;
                    }
                    let inputs = args
                        .iter()
                        .map(|arg| Atom::operand(&locals, arg))
                        .collect::<Option<_>>()?;
                    let callee = Instance {
                        function: *func,
                        args: generic_args,
                    };
                    locals[dest.index()] = Some(self.evaluate(callee, inputs, depth + 1)?);
                    *target
                }
                _ => return None,
            };
        }
        None
    }
}

fn string_table(table: ConstValue) -> Option<BTreeMap<usize, Vec<Vec<u8>>>> {
    let entries = match &table {
        ConstValue::Ref(allocation) => allocation.as_ref(),
        value => value,
    };
    let ConstValue::Aggregate(entries) = entries else {
        return None;
    };
    if !TABLE_SIZE.contains(&entries.len()) {
        return None;
    }
    let mut lengths: BTreeMap<usize, Vec<Vec<u8>>> = BTreeMap::new();
    for entry in entries.iter() {
        let ConstValue::Ref(entry) = entry else {
            return None;
        };
        let ConstValue::Aggregate(bytes) = entry.as_ref() else {
            return None;
        };
        if bytes.len() > MAX_STRING_BYTES {
            return None;
        }
        let bytes = bytes
            .iter()
            .map(|byte| match byte {
                ConstValue::Int {
                    bits,
                    ty: IntTy::U8,
                } => u8::try_from(*bits).ok(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        lengths.entry(bytes.len()).or_default().push(bytes);
    }
    for words in lengths.values_mut() {
        words.sort_unstable();
        words.dedup();
    }
    Some(lengths)
}

fn lookup(name: String, sig: Signature, lengths: BTreeMap<usize, Vec<Vec<u8>>>) -> Function {
    let mut function = Function::new(name, sig);
    function.local = true;
    function.attrs.nounwind = true;
    function.attrs.noinline = true;
    let mut b = FunctionBuilder::new(&mut function);
    let entry = b.create_block();
    let hit = b.create_block();
    let miss = b.create_block();
    b.switch_to_block(entry);
    let data = b.append_block_param(entry, Type::Ptr);
    let len = b.append_block_param(entry, Type::I64);
    let cases: Vec<_> = lengths
        .keys()
        .map(|&len| (len as i64, b.create_block()))
        .collect();
    switch(&mut b, len, miss, &cases);
    for ((size, words), (_, start)) in lengths.into_iter().zip(cases) {
        b.switch_to_block(start);
        if size == 0 {
            b.ins().jump(hit, &[]);
        } else if words.len() < 4 {
            compare_words(&mut b, data, hit, miss, &words);
        } else {
            let offset = (0..size)
                .max_by_key(|&offset| {
                    let mut seen = [false; 256];
                    for word in &words {
                        seen[usize::from(word[offset])] = true;
                    }
                    seen.into_iter().filter(|&present| present).count()
                })
                .expect("nonempty strings have a discriminating offset");
            let mut groups: BTreeMap<u8, Vec<Vec<u8>>> = BTreeMap::new();
            for word in words {
                groups.entry(word[offset]).or_default().push(word);
            }
            let value = load(&mut b, data, offset, Type::I8);
            let value = b.ins().convert(ConvertOp::Uext, Type::I64, value);
            let cases: Vec<_> = groups
                .keys()
                .map(|&byte| (i64::from(byte), b.create_block()))
                .collect();
            switch(&mut b, value, miss, &cases);
            for ((_, words), (_, start)) in groups.into_iter().zip(cases) {
                b.switch_to_block(start);
                compare_words(&mut b, data, hit, miss, &words);
            }
        }
    }
    for (block, result) in [(hit, 1), (miss, 0)] {
        b.switch_to_block(block);
        let result = b.ins().iconst(Type::I8, result);
        b.ins().return_(&[result]);
    }
    b.seal_all_blocks();
    b.finalize();
    function
}

fn switch(b: &mut FunctionBuilder<'_>, value: Value, miss: Block, cases: &[(i64, Block)]) {
    let block_call = |block| BlockCall {
        block,
        args: Vec::new(),
    };
    let cases = cases
        .iter()
        .map(|&(key, target)| (key, block_call(target)))
        .collect();
    b.ins().switch(value, block_call(miss), cases);
}

fn load(b: &mut FunctionBuilder<'_>, data: Value, offset: usize, ty: Type) -> Value {
    let pointer = if offset == 0 {
        data
    } else {
        let offset = b.ins().iconst(Type::I64, offset as i64);
        b.ins().ptradd(data, offset)
    };
    let flags = MemoryFlags {
        align: 1,
        ..MemoryFlags::default()
    };
    b.ins().load_with_flags(ty, pointer, flags)
}

fn compare_words(
    b: &mut FunctionBuilder<'_>,
    data: Value,
    hit: Block,
    miss: Block,
    words: &[Vec<u8>],
) {
    for (index, word) in words.iter().enumerate() {
        let no = if index + 1 == words.len() {
            miss
        } else {
            b.create_block()
        };
        let mut equal = None;
        let mut offset = 0;
        for (width, ty) in [
            (8, Type::I64),
            (4, Type::I32),
            (2, Type::I16),
            (1, Type::I8),
        ] {
            while offset + width <= word.len() {
                let actual = load(b, data, offset, ty);
                let sign = if word[offset + width - 1] >= 128 {
                    255
                } else {
                    0
                };
                let mut bytes = [sign; 8];
                bytes[..width].copy_from_slice(&word[offset..offset + width]);
                let expected = b.ins().iconst(ty, i64::from_le_bytes(bytes));
                let test = b.ins().icmp(IntCC::Eq, actual, expected);
                equal = Some(match equal {
                    Some(old) => b.ins().and(old, test),
                    None => test,
                });
                offset += width;
            }
        }
        b.ins().brif(
            equal.expect("empty strings are matched by length alone"),
            hit,
            &[],
            no,
            &[],
        );
        if no != miss {
            b.switch_to_block(no);
        }
    }
}
