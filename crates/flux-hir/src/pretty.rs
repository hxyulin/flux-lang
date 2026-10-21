//! Prints HIR as Rust-like text for snapshot tests: every binary and unary
//! expression in parentheses, locals as `name@index`, and loops numbered
//! so that `break#3` names the `loop#3` it leaves.

use std::fmt::Write;

use flux_syntax::ast::{ArithOp, BinaryOp, CmpOp, LogicOp, UnaryOp};

use crate::{
    Body, Diagnostic, Expr, ExprId, Function, Hir, Literal, LoopSource, Pat, PatId, Res, Stmt,
    StructKind, TypeRef,
};

pub(crate) fn file(hir: &Hir) -> String {
    let mut out = String::new();
    let items = &hir.items;
    for s in items
        .structs
        .values()
        .filter(|s| items.in_main_crate(s.loc.module))
    {
        let fields: Vec<String> = s
            .fields
            .iter()
            .map(|f| match s.kind {
                StructKind::Tuple => type_ref(&f.ty),
                _ => format!("{}: {}", f.name, type_ref(&f.ty)),
            })
            .collect();
        let fields = fields.join(", ");
        match s.kind {
            StructKind::Record => writeln!(out, "struct {}{} {{ {fields} }}", s.name, s.generics),
            StructKind::Tuple => writeln!(out, "struct {}{}({fields});", s.name, s.generics),
            StructKind::Unit => writeln!(out, "struct {}{};", s.name, s.generics),
        }
        .unwrap();
        out.push('\n');
    }
    let functions = items
        .functions
        .iter()
        .filter(|(_, f)| items.in_main_crate(f.loc.module));
    for (id, function) in functions {
        match hir.bodies.get(id) {
            Some((body, _)) => {
                let mut p = Printer {
                    hir,
                    body,
                    out: &mut out,
                    indent: 0,
                };
                p.function(function);
            }
            None => {
                let mut params: Vec<String> =
                    function.params.iter().map(|p| type_ref(&p.ty)).collect();
                if function.is_variadic {
                    params.push("...".to_owned());
                }
                let unsafety = if function.is_unsafe { "unsafe " } else { "" };
                let external = if function.is_extern { "extern " } else { "" };
                writeln!(
                    out,
                    "{unsafety}{external}fn {}({}) -> {};",
                    function.name,
                    params.join(", "),
                    type_ref(&function.ret)
                )
                .unwrap();
            }
        }
        out.push('\n');
    }
    write_diagnostics(&mut out, hir, &hir.diagnostics);
    out
}

/// `error 12..34: message`, with the file's number for files other than
/// the program's root.
pub(crate) fn write_diagnostics(out: &mut String, hir: &Hir, diagnostics: &[Diagnostic]) {
    let items = &hir.items;
    let root = items.modules[items.module_of(items.main_crate())].file;
    for d in diagnostics {
        if d.file == root {
            writeln!(out, "error {:?}: {}", d.range, d.message).unwrap();
        } else {
            writeln!(out, "error file{} {:?}: {}", d.file.0, d.range, d.message).unwrap();
        }
    }
}

fn type_ref(ty: &TypeRef) -> String {
    ty.to_string()
}

struct Printer<'a> {
    hir: &'a Hir,
    body: &'a Body,
    out: &'a mut String,
    indent: usize,
}

impl Printer<'_> {
    fn function(&mut self, function: &Function) {
        let unsafety = if function.is_unsafe { "unsafe " } else { "" };
        let name = self.hir.items.function_path(function);
        let generics = &function.generics;
        write!(self.out, "{unsafety}fn {name}{generics}(").unwrap();
        for (i, &pat) in self.body.params.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.pat(pat);
            let ty = function
                .params
                .get(i)
                .map_or(TypeRef::Error, |p| p.ty.clone());
            write!(self.out, ": {}", type_ref(&ty)).unwrap();
        }
        write!(self.out, ") -> {} ", type_ref(&function.ret)).unwrap();
        self.expr(self.body.root);
        self.out.push('\n');
    }

    /// The generic arguments written in expression `id`: `::<A>` for the
    /// item, then `[Self<B>]` for its type's, if any.
    fn generic_args(&mut self, id: ExprId) {
        let Some(generics) = self.body.generic_args.get(id) else {
            return;
        };
        let list = |types: &[crate::TypeRefId]| {
            let types: Vec<String> = types
                .iter()
                .map(|&t| type_ref(&self.body.types[t]))
                .collect();
            types.join(", ")
        };
        let (item, ty) = (
            generics.item.as_ref().map(|a| list(&a.types)),
            generics.ty.as_ref().map(|a| list(&a.types)),
        );
        if let Some(item) = item {
            write!(self.out, "::<{item}>").unwrap();
        }
        if let Some(ty) = ty {
            write!(self.out, "[Self<{ty}>]").unwrap();
        }
    }

    fn newline(&mut self) {
        self.out.push('\n');
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn pat(&mut self, pat: PatId) {
        match &self.body.pats[pat] {
            Pat::Missing => self.out.push_str("<missing>"),
            Pat::Wild => self.out.push('_'),
            Pat::Bind(binding) => {
                let b = &self.body.bindings[*binding];
                let mutability = if b.mutable { "mut " } else { "" };
                write!(self.out, "{mutability}{}@{}", b.name, binding.index()).unwrap();
            }
            Pat::Tuple { pats, rest } => {
                self.out.push('(');
                self.tuple_pats(pats, *rest);
                if pats.len() == 1 && rest.is_none() {
                    self.out.push(',');
                }
                self.out.push(')');
            }
            Pat::TupleStruct { strukt, pats, rest } => {
                write!(self.out, "{}(", self.hir.items.structs[*strukt].name).unwrap();
                self.tuple_pats(pats, *rest);
                self.out.push(')');
            }
            Pat::Record {
                strukt,
                fields,
                rest,
            } => {
                write!(self.out, "{} {{ ", self.hir.items.structs[*strukt].name).unwrap();
                for (i, (name, pat)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    write!(self.out, "{name}: ").unwrap();
                    self.pat(*pat);
                }
                if *rest {
                    self.out
                        .push_str(if fields.is_empty() { ".." } else { ", .." });
                }
                self.out.push_str(" }");
            }
            Pat::Unit(strukt) => {
                write!(self.out, "{}", self.hir.items.structs[*strukt].name).unwrap();
            }
        }
    }

    fn tuple_pats(&mut self, pats: &[PatId], rest: Option<u32>) {
        let mut first = true;
        for i in 0..=pats.len() {
            if rest == Some(i as u32) {
                self.out.push_str(if first { ".." } else { ", .." });
                first = false;
            }
            if let Some(&pat) = pats.get(i) {
                if !first {
                    self.out.push_str(", ");
                }
                self.pat(pat);
                first = false;
            }
        }
    }

    fn exprs(&mut self, exprs: &[ExprId]) {
        for (i, &e) in exprs.iter().enumerate() {
            if i > 0 {
                self.out.push_str(", ");
            }
            self.expr(e);
        }
    }

    fn expr(&mut self, id: ExprId) {
        match &self.body.exprs[id] {
            Expr::Missing => self.out.push_str("<missing>"),
            Expr::Literal(Literal::Int { value, suffix }) => {
                write!(self.out, "{value}").unwrap();
                if let Some(suffix) = suffix {
                    write!(self.out, "{suffix}").unwrap();
                }
            }
            Expr::Literal(Literal::Bool(value)) => write!(self.out, "{value}").unwrap(),
            Expr::Literal(Literal::CStr(bytes)) => {
                write!(self.out, "c\"{}\"", bytes.escape_ascii()).unwrap();
            }
            Expr::Path(Res::Local(binding)) => {
                let name = self.body.bindings[*binding].name;
                write!(self.out, "{name}@{}", binding.index()).unwrap();
            }
            Expr::Path(Res::Function(f)) => {
                let function = &self.hir.items.functions[*f];
                write!(self.out, "{}", self.hir.items.function_path(function)).unwrap();
                self.generic_args(id);
            }
            Expr::Path(Res::Struct(s)) => {
                write!(self.out, "{}", self.hir.items.structs[*s].name).unwrap();
                self.generic_args(id);
            }
            Expr::Path(Res::Const(c)) => {
                let name = self.hir.items.consts[*c].name;
                write!(
                    self.out,
                    "{}",
                    name.map_or("_".to_owned(), |n| n.to_string())
                )
                .unwrap();
            }
            Expr::Path(Res::Err) => self.out.push_str("<error>"),
            Expr::Tuple(fields) => {
                self.out.push('(');
                self.exprs(fields);
                if fields.len() == 1 {
                    self.out.push(',');
                }
                self.out.push(')');
            }
            Expr::Block {
                stmts,
                tail,
                is_unsafe,
            } => {
                if *is_unsafe {
                    self.out.push_str("unsafe ");
                }
                if stmts.is_empty() && tail.is_none() {
                    self.out.push_str("{}");
                    return;
                }
                self.out.push('{');
                self.indent += 1;
                for stmt in stmts.iter() {
                    self.newline();
                    match stmt {
                        Stmt::Let { pat, ty, init } => {
                            self.out.push_str("let ");
                            self.pat(*pat);
                            if let Some(ty) = ty {
                                let ty = type_ref(&self.body.types[*ty]);
                                write!(self.out, ": {ty}").unwrap();
                            }
                            if let Some(init) = init {
                                self.out.push_str(" = ");
                                self.expr(*init);
                            }
                            self.out.push(';');
                        }
                        Stmt::Expr {
                            expr,
                            has_semicolon,
                        } => {
                            self.expr(*expr);
                            if *has_semicolon {
                                self.out.push(';');
                            }
                        }
                    }
                }
                if let Some(tail) = tail {
                    self.newline();
                    self.expr(*tail);
                }
                self.indent -= 1;
                self.newline();
                self.out.push('}');
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.out.push_str("if ");
                self.expr(*condition);
                self.out.push(' ');
                self.expr(*then_branch);
                if let Some(else_branch) = else_branch {
                    self.out.push_str(" else ");
                    self.expr(*else_branch);
                }
            }
            Expr::Loop { body, source } => {
                write!(self.out, "loop#{} ", id.index()).unwrap();
                if *source == LoopSource::While {
                    self.out.push_str("/* while */ ");
                }
                self.expr(*body);
            }
            Expr::Break { value, target } => {
                self.out.push_str("break");
                if let Some(target) = target {
                    write!(self.out, "#{}", target.index()).unwrap();
                }
                if let Some(value) = value {
                    self.out.push(' ');
                    self.expr(*value);
                }
            }
            Expr::Continue { target } => {
                self.out.push_str("continue");
                if let Some(target) = target {
                    write!(self.out, "#{}", target.index()).unwrap();
                }
            }
            Expr::Return { value } => {
                self.out.push_str("return");
                if let Some(value) = value {
                    self.out.push(' ');
                    self.expr(*value);
                }
            }
            Expr::Call { callee, args } => {
                self.expr(*callee);
                self.out.push('(');
                self.exprs(args);
                self.out.push(')');
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.expr(*receiver);
                write!(self.out, ".{method}").unwrap();
                self.generic_args(id);
                self.out.push('(');
                self.exprs(args);
                self.out.push(')');
            }
            Expr::Unary { op, operand } => {
                let op = match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::Not => "!",
                    UnaryOp::Deref => "*",
                };
                write!(self.out, "({op}").unwrap();
                self.expr(*operand);
                self.out.push(')');
            }
            Expr::Binary { op, lhs, rhs } => {
                self.out.push('(');
                self.expr(*lhs);
                write!(self.out, " {} ", binary_op(*op)).unwrap();
                self.expr(*rhs);
                self.out.push(')');
            }
            Expr::Cast { expr, ty } => {
                self.out.push('(');
                self.expr(*expr);
                write!(self.out, " as {})", type_ref(&self.body.types[*ty])).unwrap();
            }
            Expr::Ref { mutable, expr } => {
                self.out.push_str(if *mutable { "(&mut " } else { "(&" });
                self.expr(*expr);
                self.out.push(')');
            }
            Expr::Field { expr, name } => {
                self.expr(*expr);
                write!(self.out, ".{name}").unwrap();
            }
            Expr::Record {
                strukt,
                fields,
                spread,
            } => {
                write!(self.out, "{}", self.hir.items.structs[*strukt].name).unwrap();
                self.generic_args(id);
                self.out.push_str(" { ");
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.out.push_str(", ");
                    }
                    write!(self.out, "{name}: ").unwrap();
                    self.expr(*value);
                }
                if let Some(spread) = spread {
                    self.out
                        .push_str(if fields.is_empty() { ".." } else { ", .." });
                    self.expr(*spread);
                }
                self.out.push_str(" }");
            }
        }
    }
}

fn binary_op(op: BinaryOp) -> String {
    let arith = |op: ArithOp| match op {
        ArithOp::Add => "+",
        ArithOp::Sub => "-",
        ArithOp::Mul => "*",
        ArithOp::Div => "/",
        ArithOp::Rem => "%",
        ArithOp::Shl => "<<",
        ArithOp::Shr => ">>",
        ArithOp::BitXor => "^",
        ArithOp::BitOr => "|",
        ArithOp::BitAnd => "&",
    };
    match op {
        BinaryOp::Logic(LogicOp::And) => "&&".to_owned(),
        BinaryOp::Logic(LogicOp::Or) => "||".to_owned(),
        BinaryOp::Arith(op) => arith(op).to_owned(),
        BinaryOp::Cmp(op) => match op {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
        .to_owned(),
        BinaryOp::Assign(None) => "=".to_owned(),
        BinaryOp::Assign(Some(op)) => format!("{}=", arith(op)),
    }
}
