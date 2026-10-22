//! Prints MIR in a form close to rustc's `-Zdump-mir`.

use std::fmt::Write;

use flux_hir::{Hir, StructKind};
use flux_ty::{Mutability, Ty};

use crate::{
    AggregateKind, BinOp, Body, Constant, FileMir, LocalKind, Operand, Place, ProjectionElem,
    Rvalue, StatementKind, TerminatorKind, UnOp,
};

pub(crate) fn file(mir: &FileMir, hir: &Hir) -> String {
    let mut out = String::new();
    for (id, body) in mir.bodies.iter().filter(|&(id, _)| hir.is_program(id)) {
        let function = &hir.items.functions[id];
        let name = format!("{}{}", hir.items.function_path(function), function.generics);
        body_text(&mut out, hir, &mir.discriminants, &name, body);
        out.push('\n');
    }
    hir.write_diagnostics(&mut out, &mir.diagnostics);
    out
}

fn body_text(
    out: &mut String,
    hir: &Hir,
    discriminants: &crate::Discriminants,
    name: &str,
    body: &Body,
) {
    let params: Vec<String> = body
        .params()
        .map(|p| format!("_{}: {}", p.index(), body.locals[p].ty))
        .collect();
    writeln!(
        out,
        "fn {name}({}) -> {} {{",
        params.join(", "),
        body.locals[Body::RETURN].ty
    )
    .unwrap();
    for (local, decl) in body.locals.iter() {
        if body.params().any(|p| p == local) {
            continue;
        }
        let mutability = match decl.kind {
            LocalKind::User { mutable: false, .. } | LocalKind::Temp => "",
            _ => "mut ",
        };
        write!(out, "    let {mutability}_{}: {};", local.index(), decl.ty).unwrap();
        if let LocalKind::User { name, .. } = decl.kind {
            write!(out, " // {name}").unwrap();
        }
        out.push('\n');
    }
    for (i, bytes) in body.c_strings.iter().enumerate() {
        writeln!(
            out,
            "    const c_str{i}: &CStr = c\"{}\";",
            bytes.escape_ascii()
        )
        .unwrap();
    }
    for (block, data) in body.blocks.iter() {
        writeln!(out, "\n    bb{}: {{", block.index()).unwrap();
        for statement in &data.statements {
            match &statement.kind {
                StatementKind::Assign(dest, rvalue) => writeln!(
                    out,
                    "        {} = {};",
                    place(dest),
                    rvalue_text(hir, rvalue)
                ),
                StatementKind::StorageDead(local) => {
                    writeln!(out, "        StorageDead(_{});", local.index())
                }
                StatementKind::PlaceMention(p) => {
                    writeln!(out, "        PlaceMention({});", place(p))
                }
            }
            .unwrap();
        }
        let terminator = match &data.terminator.kind {
            TerminatorKind::Goto(target) => format!("goto -> bb{}", target.index()),
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
                variants,
            } => {
                let mut arms: Vec<String> = targets
                    .iter()
                    .map(|&(value, bb)| {
                        // A variant's discriminant, or its path if that
                        // is not known.
                        let value = match variants {
                            Some(strukt) => crate::discriminant_bits(
                                &hir.items,
                                discriminants,
                                *strukt,
                                value as u32,
                            )
                            .map_or_else(
                                || hir.items.structs[*strukt].variant(value as u32).path(),
                                |bits| bits.to_string(),
                            ),
                            None => value.to_string(),
                        };
                        format!("{value}: bb{}", bb.index())
                    })
                    .collect();
                arms.push(format!("otherwise: bb{}", otherwise.index()));
                format!(
                    "switchInt({}) -> [{}]",
                    operand(hir, discr),
                    arms.join(", ")
                )
            }
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } => {
                let args: Vec<String> = args.iter().map(|o| operand(hir, o)).collect();
                let mut name = hir.items.functions[*func].name.to_string();
                if !generic_args.is_empty() {
                    let types: Vec<String> = generic_args.iter().map(Ty::to_string).collect();
                    name = format!("{name}::<{}>", types.join(", "));
                }
                let target = match target {
                    Some(bb) => format!("bb{}", bb.index()),
                    None => "!".to_owned(),
                };
                format!("{} = {name}({}) -> {target}", place(dest), args.join(", "))
            }
            TerminatorKind::Assert {
                cond,
                expected,
                msg,
                target,
            } => {
                let not = if *expected { "" } else { "!" };
                format!(
                    "assert({not}{}, \"{}\") -> bb{}",
                    operand(hir, cond),
                    msg.text(),
                    target.index()
                )
            }
            TerminatorKind::Drop { place: p, target } => {
                format!("drop({}) -> bb{}", place(p), target.index())
            }
            TerminatorKind::Return => "return".to_owned(),
            TerminatorKind::Unreachable => "unreachable".to_owned(),
        };
        writeln!(out, "        {terminator};\n    }}").unwrap();
    }
    out.push_str("}\n");
}

fn rvalue_text(hir: &Hir, rvalue: &Rvalue) -> String {
    match rvalue {
        Rvalue::Use(op) => operand(hir, op),
        Rvalue::BinaryOp(op, l, r) => {
            let op = match op {
                BinOp::Add => "Add",
                BinOp::Sub => "Sub",
                BinOp::Mul => "Mul",
                BinOp::Div => "Div",
                BinOp::Rem => "Rem",
                BinOp::Shl => "Shl",
                BinOp::Shr => "Shr",
                BinOp::BitAnd => "BitAnd",
                BinOp::BitOr => "BitOr",
                BinOp::BitXor => "BitXor",
                BinOp::Eq => "Eq",
                BinOp::Ne => "Ne",
                BinOp::Lt => "Lt",
                BinOp::Le => "Le",
                BinOp::Gt => "Gt",
                BinOp::Ge => "Ge",
            };
            format!("{op}({}, {})", operand(hir, l), operand(hir, r))
        }
        Rvalue::UnaryOp(op, o) => {
            let op = match op {
                UnOp::Neg => "Neg",
                UnOp::Not => "Not",
            };
            format!("{op}({})", operand(hir, o))
        }
        Rvalue::Cast(o, ty) => format!("{} as {ty}", operand(hir, o)),
        Rvalue::Ref {
            mutability,
            place: p,
            two_phase,
        } => {
            let kind = match (mutability, two_phase) {
                (Mutability::Not, _) => "&",
                (Mutability::Mut, false) => "&mut ",
                (Mutability::Mut, true) => "&two_phase ",
            };
            format!("{kind}{}", place(p))
        }
        Rvalue::Aggregate(AggregateKind::Tuple, ops) => {
            let ops: Vec<String> = ops.iter().map(|o| operand(hir, o)).collect();
            match ops.as_slice() {
                [one] => format!("({one},)"),
                _ => format!("({})", ops.join(", ")),
            }
        }
        Rvalue::Aggregate(AggregateKind::Struct(strukt), ops) => {
            aggregate(hir, hir.items.structs[*strukt].variant(0), ops)
        }
        Rvalue::Aggregate(AggregateKind::Variant(strukt, index), ops) => {
            aggregate(hir, hir.items.structs[*strukt].variant(*index), ops)
        }
        Rvalue::Aggregate(AggregateKind::Array(_), ops) => {
            let ops: Vec<String> = ops.iter().map(|o| operand(hir, o)).collect();
            format!("[{}]", ops.join(", "))
        }
        Rvalue::Discriminant(p) => format!("discriminant({})", place(p)),
        Rvalue::Len {
            place: p,
            borrows: true,
        } => format!("Len(&{})", place(p)),
        Rvalue::Len {
            place: p,
            borrows: false,
        } => format!("Len({})", place(p)),
        Rvalue::Repeat(o, count) => format!("[{}; {count}]", operand(hir, o)),
        Rvalue::Unsize(o, ty) => format!("{} as {ty} (unsize)", operand(hir, o)),
    }
}

/// A struct or variant built from `ops`: `S { a: .. }`, `E::B(..)`.
fn aggregate(hir: &Hir, v: flux_hir::VariantData<'_>, ops: &[Operand]) -> String {
    let name = v.path();
    match v.kind {
        StructKind::Unit => name,
        StructKind::Tuple => {
            let ops: Vec<String> = ops.iter().map(|o| operand(hir, o)).collect();
            format!("{name}({})", ops.join(", "))
        }
        StructKind::Record => {
            let fields: Vec<String> = v
                .fields
                .iter()
                .zip(ops)
                .map(|(f, op)| format!("{}: {}", f.name, operand(hir, op)))
                .collect();
            format!("{name} {{ {} }}", fields.join(", "))
        }
    }
}

/// `_1`, or `_1.0.2` for fields, by index.
fn place(place: &Place) -> String {
    let mut text = format!("_{}", place.local.index());
    for elem in &place.projection {
        match elem {
            ProjectionElem::Field(field) => text.push_str(&format!(".{field}")),
            ProjectionElem::Deref => text = format!("(*{text})"),
            ProjectionElem::Downcast(variant) => text = format!("({text} as #{variant})"),
            ProjectionElem::Index(local) => text.push_str(&format!("[_{}]", local.index())),
        }
    }
    text
}

fn operand(hir: &Hir, operand: &Operand) -> String {
    match operand {
        Operand::Copy(p) => format!("copy {}", place(p)),
        Operand::Move(p) => format!("move {}", place(p)),
        Operand::Const(constant) => format!("const {}", constant_text(hir, *constant)),
    }
}

fn constant_text(hir: &Hir, constant: Constant) -> String {
    match constant {
        Constant::Int { bits, ty } => {
            let width = ty.bits();
            let value = if ty.is_signed() && width < 128 && bits >> (width - 1) & 1 == 1 {
                // Sign-extend to show the negative value.
                ((bits as i128) - (1i128 << width)).to_string()
            } else if ty.is_signed() && width == 128 {
                (bits as i128).to_string()
            } else {
                bits.to_string()
            };
            format!("{value}_{}", Ty::int(ty))
        }
        Constant::Bool(value) => value.to_string(),
        Constant::Unit => "()".to_owned(),
        Constant::CStr(index) => format!("c_str{index}"),
        Constant::Item { id, args, .. } => {
            let name = hir.items.const_name(id);
            let items = &hir.items;
            match items.consts[id].owner {
                // `<T as Tr>::N`
                Some(flux_hir::Owner::Trait(t)) if !args.is_empty() => {
                    let trait_args: Vec<String> = args[1..].iter().map(|a| a.to_string()).collect();
                    let trait_args = if trait_args.is_empty() {
                        String::new()
                    } else {
                        format!("<{}>", trait_args.join(", "))
                    };
                    format!(
                        "<{} as {}{trait_args}>::{name}",
                        args[0], items.traits[t].name
                    )
                }
                // `W::N::<T>`, with the block's arguments.
                Some(flux_hir::Owner::Impl(imp)) if !args.is_empty() => {
                    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    let owner = match items.impls[imp].target {
                        Some(flux_hir::ImplTarget::Struct(s)) => {
                            format!("{}::", items.structs[s].name)
                        }
                        _ => String::new(),
                    };
                    format!("{owner}{name}::<{}>", args.join(", "))
                }
                _ => name,
            }
        }
    }
}
