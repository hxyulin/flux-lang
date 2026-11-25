//! Recover scalar products when only a limb of a widened product is read.
use keel_ir::{BinaryOp, ConvertOp, Function, InstData, Module, Type, Value, ValueDef};

pub(super) fn run(module: &mut Module) -> bool {
    let mut changed = false;
    for function in &mut module.functions {
        if function.attrs.optnone || function.attrs.naked || function.attrs.interrupt {
            continue;
        }
        let rewrites: Vec<_> = function
            .layout_insts()
            .filter_map(|(_, inst)| {
                let &InstData::Convert {
                    op: ConvertOp::Trunc,
                    ty: Type::I64,
                    arg,
                } = function.inst_data(inst)
                else {
                    return None;
                };
                let (product, high) = match definition(function, arg)? {
                    &InstData::Binary {
                        op: BinaryOp::Ushr | BinaryOp::Sshr,
                        args: [product, amount],
                    } if matches!(definition(function, amount),
                        Some(InstData::Iconst { imm, .. }) if imm & 127 == 64) =>
                    {
                        (product, true)
                    }
                    _ => (arg, false),
                };
                let &InstData::Binary {
                    op: BinaryOp::Imul,
                    args,
                } = definition(function, product)?
                else {
                    return None;
                };
                let extend = |value| match definition(function, value)? {
                    &InstData::Convert {
                        op: op @ (ConvertOp::Uext | ConvertOp::Sext),
                        ty: Type::I128,
                        arg,
                    } if function.value_type(arg) == Type::I64 => Some((op, arg)),
                    _ => None,
                };
                let (left, a) = extend(args[0])?;
                let (right, b) = extend(args[1])?;
                let op = match (high, left, right) {
                    (false, _, _) => BinaryOp::Imul,
                    (true, ConvertOp::Uext, ConvertOp::Uext) => BinaryOp::Umulhi,
                    (true, ConvertOp::Sext, ConvertOp::Sext) => BinaryOp::Smulhi,
                    _ => return None,
                };
                Some((inst, InstData::Binary { op, args: [a, b] }))
            })
            .collect();
        changed |= !rewrites.is_empty();
        for (inst, replacement) in rewrites {
            function.replace_inst(inst, replacement);
        }
    }
    changed
}

fn definition(function: &Function, value: Value) -> Option<&InstData> {
    let ValueDef::Result(inst, 0) = function.value_def(function.resolve(value)) else {
        return None;
    };
    Some(function.inst_data(inst))
}
