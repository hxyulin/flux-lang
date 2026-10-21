//! Prints MIR in a form close to rustc's `-Zdump-mir`.

use std::fmt::Write;

use flux_hir::FileHir;
use flux_ty::Ty;

use crate::{
    BinOp, Body, Constant, FileMir, LocalKind, Operand, Rvalue, StatementKind, TerminatorKind, UnOp,
};

pub(crate) fn file(mir: &FileMir, hir: &FileHir) -> String {
    let mut out = String::new();
    for (id, body) in mir.bodies.iter() {
        let name = hir.items.functions[id].name;
        body_text(&mut out, hir, &name.to_string(), body);
        out.push('\n');
    }
    for d in &mir.diagnostics {
        writeln!(out, "error {:?}: {}", d.range, d.message).unwrap();
    }
    out
}

fn body_text(out: &mut String, hir: &FileHir, name: &str, body: &Body) {
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
        if matches!(decl.kind, LocalKind::Param) {
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
    for (block, data) in body.blocks.iter() {
        writeln!(out, "\n    bb{}: {{", block.index()).unwrap();
        for statement in &data.statements {
            let StatementKind::Assign(dest, rvalue) = &statement.kind;
            writeln!(out, "        _{} = {};", dest.index(), rvalue_text(rvalue)).unwrap();
        }
        let terminator = match &data.terminator.kind {
            TerminatorKind::Goto(target) => format!("goto -> bb{}", target.index()),
            TerminatorKind::SwitchInt {
                discr,
                targets,
                otherwise,
            } => {
                let mut arms: Vec<String> = targets
                    .iter()
                    .map(|(value, bb)| format!("{value}: bb{}", bb.index()))
                    .collect();
                arms.push(format!("otherwise: bb{}", otherwise.index()));
                format!("switchInt({}) -> [{}]", operand(discr), arms.join(", "))
            }
            TerminatorKind::Call {
                func,
                args,
                dest,
                target,
            } => {
                let args: Vec<String> = args.iter().map(operand).collect();
                let name = hir.items.functions[*func].name;
                let target = match target {
                    Some(bb) => format!("bb{}", bb.index()),
                    None => "!".to_owned(),
                };
                format!(
                    "_{} = {name}({}) -> {target}",
                    dest.index(),
                    args.join(", ")
                )
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
                    operand(cond),
                    msg.text(),
                    target.index()
                )
            }
            TerminatorKind::Return => "return".to_owned(),
            TerminatorKind::Unreachable => "unreachable".to_owned(),
        };
        writeln!(out, "        {terminator};\n    }}").unwrap();
    }
    out.push_str("}\n");
}

fn rvalue_text(rvalue: &Rvalue) -> String {
    match rvalue {
        Rvalue::Use(op) => operand(op),
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
            format!("{op}({}, {})", operand(l), operand(r))
        }
        Rvalue::UnaryOp(op, o) => {
            let op = match op {
                UnOp::Neg => "Neg",
                UnOp::Not => "Not",
            };
            format!("{op}({})", operand(o))
        }
        Rvalue::Cast(o, ty) => format!("{} as {ty}", operand(o)),
    }
}

fn operand(operand: &Operand) -> String {
    match operand {
        Operand::Copy(local) => format!("copy _{}", local.index()),
        Operand::Const(constant) => format!("const {}", constant_text(*constant)),
    }
}

fn constant_text(constant: Constant) -> String {
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
            format!("{value}_{}", Ty::Int(ty))
        }
        Constant::Bool(value) => value.to_string(),
        Constant::Unit => "()".to_owned(),
    }
}
