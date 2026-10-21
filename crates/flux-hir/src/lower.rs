//! Lowers a function body from the typed AST to HIR.

use flux_arena::Arena;
use flux_intern::Symbol;
use flux_syntax::ast::{self, AstNode, LiteralKind};
use flux_syntax::{SyntaxNode, SyntaxTree, TextRange};

use crate::body::{
    Binding, BindingId, Body, BodySourceMap, Expr, ExprId, Literal, LoopSource, Pat, PatId, Res,
    Stmt,
};
use crate::item_tree::{check_attr, check_attrs, lower_type, unsupported};
use crate::literal::{parse_int, unescape_c_str};
use crate::{Diagnostic, FunctionId, ItemTree, StructId, StructKind};

/// Where a pattern binds, for "cannot shadow" messages.
#[derive(Clone, Copy)]
enum PatSite {
    Let,
    Param,
}

/// A lowered body with its source map and diagnostics.
#[derive(Debug)]
pub struct LoweredBody {
    pub body: Body,
    pub source_map: BodySourceMap,
    pub diagnostics: Vec<Diagnostic>,
}

/// Lowers the body of `function`, which must have one.
pub fn lower_body(tree: &SyntaxTree, items: &ItemTree, function: FunctionId) -> LoweredBody {
    let node = tree.node(items.functions[function].node);
    let f = ast::Fn::cast(node).expect("functions point at FN nodes");
    let mut ctx = Ctx {
        items,
        exprs: Arena::new(),
        pats: Arena::new(),
        bindings: Arena::new(),
        types: Arena::new(),
        source_map: BodySourceMap::default(),
        scopes: Vec::new(),
        loops: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut params = Vec::new();
    if let Some(list) = f.param_list() {
        for param in list.params() {
            params.push(ctx.lower_pat(param.pat(), param.syntax(), PatSite::Param));
        }
    }
    let root = match f.body() {
        Some(block) => ctx.lower_block(block),
        None => ctx.alloc_expr(Expr::Missing, f.syntax().text_range()),
    };
    LoweredBody {
        body: Body {
            exprs: ctx.exprs,
            pats: ctx.pats,
            bindings: ctx.bindings,
            types: ctx.types,
            params: params.into(),
            root,
        },
        source_map: ctx.source_map,
        diagnostics: ctx.diagnostics,
    }
}

struct Ctx<'a> {
    items: &'a ItemTree,
    exprs: Arena<Expr>,
    pats: Arena<Pat>,
    bindings: Arena<Binding>,
    types: Arena<crate::TypeRef>,
    source_map: BodySourceMap,
    /// Bindings in scope, innermost last; a block truncates it on exit.
    scopes: Vec<(Symbol, BindingId)>,
    /// The enclosing loops, innermost last.
    loops: Vec<ExprId>,
    diagnostics: Vec<Diagnostic>,
}

impl Ctx<'_> {
    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.source_map.exprs.insert(id, range);
        id
    }

    fn alloc_pat(&mut self, pat: Pat, range: TextRange) -> PatId {
        let id = self.pats.alloc(pat);
        self.source_map.pats.insert(id, range);
        id
    }

    fn unsupported(&mut self, range: TextRange, what: &str) {
        unsupported(&mut self.diagnostics, range, what);
    }

    fn error(&mut self, range: TextRange, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(range, message));
    }

    /// Attributes on an expression node.
    fn check_attrs(&mut self, node: SyntaxNode<'_>) {
        for attr in node.children().filter_map(ast::Attr::cast) {
            check_attr(attr, &mut self.diagnostics);
        }
    }

    /// Lowers `expr`, or a `Missing` covering `parent` if the parser found
    /// none (and reported it).
    fn lower_opt(&mut self, expr: Option<ast::Expr<'_>>, parent: SyntaxNode<'_>) -> ExprId {
        match expr {
            Some(expr) => self.lower_expr(expr),
            None => self.alloc_expr(Expr::Missing, parent.text_range()),
        }
    }

    fn lower_expr(&mut self, expr: ast::Expr<'_>) -> ExprId {
        let syntax = expr.syntax();
        let range = syntax.text_range();
        // A block checks its own, inner attributes included.
        if !matches!(expr, ast::Expr::BlockExpr(_)) {
            self.check_attrs(syntax);
        }
        let hir = match expr {
            ast::Expr::Literal(literal) => self.lower_literal(literal),
            ast::Expr::PathExpr(path) => Expr::Path(self.resolve_path(path)),
            ast::Expr::ParenExpr(paren) => return self.lower_opt(paren.expr(), syntax),
            ast::Expr::TupleExpr(tuple) => {
                let fields = tuple.fields().map(|e| self.lower_expr(e)).collect();
                Expr::Tuple(fields)
            }
            ast::Expr::BlockExpr(block) => return self.lower_block(block),
            ast::Expr::IfExpr(if_expr) => self.lower_if(if_expr),
            ast::Expr::LoopExpr(loop_expr) => {
                if let Some(label) = loop_expr.label() {
                    self.unsupported(label.syntax().text_range(), "loop labels");
                }
                let id = self.alloc_expr(Expr::Missing, range);
                self.loops.push(id);
                let body = self.lower_opt_block(loop_expr.loop_body(), syntax);
                self.loops.pop();
                self.exprs[id] = Expr::Loop {
                    body,
                    source: LoopSource::Loop,
                };
                return id;
            }
            ast::Expr::WhileExpr(while_expr) => return self.lower_while(while_expr),
            ast::Expr::BreakExpr(break_expr) => {
                if let Some(label) = break_expr.lifetime() {
                    self.unsupported(label.syntax().text_range(), "loop labels");
                }
                let value = break_expr.expr().map(|e| self.lower_expr(e));
                let target = self.loop_target(range, "break");
                Expr::Break { value, target }
            }
            ast::Expr::ContinueExpr(continue_expr) => {
                if let Some(label) = continue_expr.lifetime() {
                    self.unsupported(label.syntax().text_range(), "loop labels");
                }
                let target = self.loop_target(range, "continue");
                Expr::Continue { target }
            }
            ast::Expr::ReturnExpr(return_expr) => {
                let value = return_expr.expr().map(|e| self.lower_expr(e));
                Expr::Return { value }
            }
            ast::Expr::CallExpr(call) => {
                let callee = match call.expr() {
                    // `S(1)` for a struct with named fields.
                    Some(ast::Expr::PathExpr(path)) if self.names_record_struct(path) => {
                        let name = path.path().and_then(simple_path).unwrap_or_default();
                        self.error(
                            range,
                            format!(
                                "cannot find function, tuple struct or tuple variant `{name}` in this scope"
                            ),
                        );
                        self.alloc_expr(Expr::Path(Res::Err), path.syntax().text_range())
                    }
                    callee => self.lower_opt(callee, syntax),
                };
                let args = match call.arg_list() {
                    Some(list) => list.args().map(|e| self.lower_expr(e)).collect(),
                    None => Box::default(),
                };
                Expr::Call { callee, args }
            }
            ast::Expr::PrefixExpr(prefix) => {
                let operand = self.lower_opt(prefix.expr(), syntax);
                match prefix.op_kind() {
                    Some(op) => Expr::Unary { op, operand },
                    None => Expr::Missing,
                }
            }
            ast::Expr::BinExpr(bin) => {
                let lhs = self.lower_opt(bin.lhs(), syntax);
                let rhs = self.lower_opt(bin.rhs(), syntax);
                match (bin.op_kind(), bin.op()) {
                    (Some(op), Some(token)) => {
                        let id = self.alloc_expr(Expr::Binary { op, lhs, rhs }, range);
                        self.source_map.ops.insert(id, token.text_range());
                        return id;
                    }
                    _ => Expr::Missing,
                }
            }
            ast::Expr::CastExpr(cast) => {
                let expr = self.lower_opt(cast.expr(), syntax);
                let ty_range = cast.ty().map_or(range, |t| t.syntax().text_range());
                let ty = lower_type(cast.ty(), &mut self.diagnostics);
                let ty = self.types.alloc(ty);
                self.source_map.types.insert(ty, ty_range);
                Expr::Cast { expr, ty }
            }
            ast::Expr::ArrayExpr(_) => self.unsupported_expr(range, "arrays"),
            ast::Expr::AwaitExpr(_) => self.unsupported_expr(range, "`.await` expressions"),
            ast::Expr::ClosureExpr(_) => self.unsupported_expr(range, "closures"),
            ast::Expr::FieldExpr(field) => {
                let base = self.lower_opt(field.expr(), syntax);
                match field.name_ref() {
                    Some(name) => {
                        let name_range = name.syntax().text_range();
                        let id = self.alloc_expr(
                            Expr::Field {
                                expr: base,
                                name: Symbol::intern(name.text()),
                            },
                            range,
                        );
                        self.source_map
                            .expr_fields
                            .insert(id, Box::new([name_range]));
                        return id;
                    }
                    None => Expr::Missing,
                }
            }
            ast::Expr::ForExpr(_) => self.unsupported_expr(range, "`for` loops"),
            ast::Expr::IndexExpr(_) => self.unsupported_expr(range, "index expressions"),
            ast::Expr::LetExpr(_) => self.unsupported_expr(range, "`let` conditions"),
            ast::Expr::MacroExpr(_) => self.unsupported_expr(range, "macros"),
            ast::Expr::MatchExpr(_) => self.unsupported_expr(range, "`match` expressions"),
            ast::Expr::MethodCallExpr(call) => {
                if let Some(args) = call.generic_arg_list() {
                    self.unsupported(args.syntax().text_range(), "generic arguments");
                }
                let receiver = self.lower_opt(call.receiver(), syntax);
                let args = match call.arg_list() {
                    Some(list) => list.args().map(|e| self.lower_expr(e)).collect(),
                    None => Box::default(),
                };
                match call.name_ref() {
                    Some(name) => Expr::MethodCall {
                        receiver,
                        method: Symbol::intern(name.text()),
                        args,
                    },
                    None => Expr::Missing,
                }
            }
            ast::Expr::RangeExpr(_) => self.unsupported_expr(range, "ranges"),
            ast::Expr::RecordExpr(record) => return self.lower_record(record),
            ast::Expr::RefExpr(_) => self.unsupported_expr(range, "references"),
            ast::Expr::TryExpr(_) => self.unsupported_expr(range, "`?` expressions"),
            ast::Expr::UnderscoreExpr(_) => {
                self.unsupported_expr(range, "destructuring assignments")
            }
        };
        self.alloc_expr(hir, range)
    }

    /// `S { a: 1, b, ..base }`
    fn lower_record(&mut self, record: ast::RecordExpr<'_>) -> ExprId {
        let range = record.syntax().text_range();
        let strukt = self.resolve_struct(record.path(), range);
        let mut fields = Vec::new();
        let mut name_ranges = Vec::new();
        let mut spread = None;
        if let Some(list) = record.record_expr_field_list() {
            for field in list.fields() {
                check_attrs(field.attrs(), &mut self.diagnostics);
                let Some(name) = field.name_ref() else {
                    continue;
                };
                let name_range = name.syntax().text_range();
                let symbol = Symbol::intern(name.text());
                let expr = match field.expr() {
                    Some(expr) => self.lower_expr(expr),
                    // `S { a }` is `S { a: a }`.
                    None => {
                        let res = self.resolve_name(symbol, name_range);
                        self.alloc_expr(Expr::Path(res), name_range)
                    }
                };
                fields.push((symbol, expr));
                name_ranges.push(name_range);
            }
            if list.dot2_token().is_some() {
                match list.spread() {
                    Some(base) => spread = Some(self.lower_expr(base)),
                    None => self.unsupported(range, "default field values"),
                }
            }
        }
        let Some(strukt) = strukt else {
            return self.alloc_expr(Expr::Missing, range);
        };
        let id = self.alloc_expr(
            Expr::Record {
                strukt,
                fields: fields.into(),
                spread,
            },
            range,
        );
        self.source_map.expr_fields.insert(id, name_ranges.into());
        if let Some(path) = record.path() {
            self.source_map.paths.insert(id, path.syntax().text_range());
        }
        id
    }

    /// The struct a path in a struct expression or pattern names.
    fn resolve_struct(
        &mut self,
        path: Option<ast::Path<'_>>,
        range: TextRange,
    ) -> Option<StructId> {
        let range = path.map_or(range, |p| p.syntax().text_range());
        let Some(name) = path.and_then(simple_path) else {
            self.unsupported(range, "paths like this");
            return None;
        };
        let name = Symbol::intern(name);
        match self.items.struct_by_name(name) {
            Some((id, _)) => Some(id),
            None => {
                self.error(
                    range,
                    format!("cannot find struct, variant or union type `{name}` in this scope"),
                );
                None
            }
        }
    }

    fn unsupported_expr(&mut self, range: TextRange, what: &str) -> Expr {
        self.unsupported(range, what);
        Expr::Missing
    }

    fn lower_opt_block(
        &mut self,
        block: Option<ast::BlockExpr<'_>>,
        parent: SyntaxNode<'_>,
    ) -> ExprId {
        match block {
            Some(block) => self.lower_block(block),
            None => self.alloc_expr(Expr::Missing, parent.text_range()),
        }
    }

    fn lower_block(&mut self, block: ast::BlockExpr<'_>) -> ExprId {
        let syntax = block.syntax();
        check_attrs(block.attrs(), &mut self.diagnostics);
        if let Some(label) = block.label() {
            self.unsupported(label.syntax().text_range(), "labeled blocks");
        }
        for token in [block.const_token(), block.async_token(), block.move_token()]
            .into_iter()
            .flatten()
        {
            let what = format!("`{}` blocks", token.text());
            self.unsupported(token.text_range(), &what);
        }
        let scope = self.scopes.len();
        let mut stmts = Vec::new();
        for stmt in block.stmts() {
            match stmt {
                ast::Stmt::LetStmt(let_stmt) => {
                    check_attrs(let_stmt.attrs(), &mut self.diagnostics);
                    if let Some(let_else) = let_stmt.let_else() {
                        self.unsupported(let_else.syntax().text_range(), "`let`-`else` statements");
                    }
                    // The initializer is lowered before the pattern, whose
                    // bindings it cannot see: `let x = x + 1;`.
                    let init = let_stmt.initializer().map(|e| self.lower_expr(e));
                    let ty = let_stmt.ty().map(|ty| {
                        let range = ty.syntax().text_range();
                        let ty = lower_type(Some(ty), &mut self.diagnostics);
                        let id = self.types.alloc(ty);
                        self.source_map.types.insert(id, range);
                        id
                    });
                    let pat = self.lower_pat(let_stmt.pat(), let_stmt.syntax(), PatSite::Let);
                    stmts.push(Stmt::Let { pat, ty, init });
                }
                ast::Stmt::ExprStmt(expr_stmt) => {
                    check_attrs(expr_stmt.attrs(), &mut self.diagnostics);
                    let expr = self.lower_opt(expr_stmt.expr(), expr_stmt.syntax());
                    stmts.push(Stmt::Expr {
                        expr,
                        has_semicolon: expr_stmt.semicolon_token().is_some(),
                    });
                }
                ast::Stmt::Item(item) => {
                    self.unsupported(item.syntax().text_range(), "items inside functions");
                }
            }
        }
        let tail = block.tail_expr().map(|e| self.lower_expr(e));
        self.scopes.truncate(scope);
        self.alloc_expr(
            Expr::Block {
                stmts: stmts.into(),
                tail,
                is_unsafe: block.unsafe_token().is_some(),
            },
            syntax.text_range(),
        )
    }

    fn lower_if(&mut self, if_expr: ast::IfExpr<'_>) -> Expr {
        let syntax = if_expr.syntax();
        let condition = self.lower_opt(if_expr.condition(), syntax);
        let then_branch = self.lower_opt_block(if_expr.then_branch(), syntax);
        let else_branch = if_expr.else_branch().map(|branch| match branch {
            ast::ElseBranch::Block(block) => self.lower_block(block),
            ast::ElseBranch::If(nested) => {
                let range = nested.syntax().text_range();
                let hir = self.lower_if(nested);
                self.alloc_expr(hir, range)
            }
        });
        Expr::If {
            condition,
            then_branch,
            else_branch,
        }
    }

    /// `while c { body }` is `loop { if c { body } else { break } }`; every
    /// node it adds maps to the `while`.
    fn lower_while(&mut self, while_expr: ast::WhileExpr<'_>) -> ExprId {
        let syntax = while_expr.syntax();
        let range = syntax.text_range();
        if let Some(label) = while_expr.label() {
            self.unsupported(label.syntax().text_range(), "loop labels");
        }
        let id = self.alloc_expr(Expr::Missing, range);
        self.loops.push(id);
        let condition = self.lower_opt(while_expr.condition(), syntax);
        let then_branch = self.lower_opt_block(while_expr.loop_body(), syntax);
        self.loops.pop();
        let exit = self.alloc_expr(
            Expr::Break {
                value: None,
                target: Some(id),
            },
            range,
        );
        let else_branch = self.alloc_expr(
            Expr::Block {
                stmts: Box::default(),
                tail: Some(exit),
                is_unsafe: false,
            },
            range,
        );
        let body = self.alloc_expr(
            Expr::If {
                condition,
                then_branch,
                else_branch: Some(else_branch),
            },
            range,
        );
        self.exprs[id] = Expr::Loop {
            body,
            source: LoopSource::While,
        };
        id
    }

    fn loop_target(&mut self, range: TextRange, keyword: &str) -> Option<ExprId> {
        let target = self.loops.last().copied();
        if target.is_none() {
            self.error(range, format!("`{keyword}` outside of a loop"));
        }
        target
    }

    fn resolve_path(&mut self, path: ast::PathExpr<'_>) -> Res {
        let range = path.syntax().text_range();
        let Some(name) = path.path().and_then(simple_path) else {
            self.unsupported(range, "paths like this");
            return Res::Err;
        };
        self.resolve_name(Symbol::intern(name), range)
    }

    /// Whether `path` names a struct with named fields and nothing in
    /// the value namespace.
    fn names_record_struct(&self, path: ast::PathExpr<'_>) -> bool {
        let Some(name) = path.path().and_then(simple_path) else {
            return false;
        };
        let name = Symbol::intern(name);
        !self.scopes.iter().any(|(n, _)| *n == name)
            && self.items.function_by_name(name).is_none()
            && self
                .items
                .struct_by_name(name)
                .is_some_and(|(_, s)| s.kind == StructKind::Record)
    }

    /// A name in the value namespace: a local, a function, or a tuple or
    /// unit struct.
    fn resolve_name(&mut self, name: Symbol, range: TextRange) -> Res {
        if let Some(&(_, binding)) = self.scopes.iter().rev().find(|(n, _)| *n == name) {
            return Res::Local(binding);
        }
        if let Some((function, _)) = self.items.function_by_name(name) {
            return Res::Function(function);
        }
        if let Some((id, s)) = self.items.struct_by_name(name) {
            if s.kind != StructKind::Record {
                return Res::Struct(id);
            }
            self.error(range, format!("cannot find value `{name}` in this scope"));
            return Res::Err;
        }
        self.error(range, format!("cannot find value `{name}` in this scope"));
        Res::Err
    }

    fn lower_literal(&mut self, literal: ast::Literal<'_>) -> Expr {
        let range = literal.syntax().text_range();
        let Some(token) = literal.value() else {
            return Expr::Missing;
        };
        match literal.kind() {
            Some(LiteralKind::Int) => match parse_int(token.text()) {
                Ok(literal) => Expr::Literal(literal),
                Err(message) => {
                    self.error(range, message);
                    Expr::Missing
                }
            },
            Some(LiteralKind::Bool(value)) => Expr::Literal(Literal::Bool(value)),
            Some(LiteralKind::Float) => self.unsupported_expr(range, "floating-point numbers"),
            Some(LiteralKind::Char) => self.unsupported_expr(range, "`char` literals"),
            Some(LiteralKind::Byte) => self.unsupported_expr(range, "byte literals"),
            Some(LiteralKind::CString) => match unescape_c_str(token.text()) {
                Ok(bytes) => Expr::Literal(Literal::CStr(bytes.into())),
                Err(message) => {
                    self.error(range, message);
                    Expr::Missing
                }
            },
            Some(LiteralKind::String | LiteralKind::ByteString) => {
                self.unsupported_expr(range, "string literals other than `c\"...\"`")
            }
            None => Expr::Missing,
        }
    }

    fn lower_pat(
        &mut self,
        pat: Option<ast::Pat<'_>>,
        parent: SyntaxNode<'_>,
        site: PatSite,
    ) -> PatId {
        let Some(pat) = pat else {
            return self.alloc_pat(Pat::Missing, parent.text_range());
        };
        let range = pat.syntax().text_range();
        let hir = match pat {
            ast::Pat::IdentPat(ident) => {
                if let Some(token) = ident.ref_token() {
                    self.unsupported(token.text_range(), "`ref` bindings");
                }
                if let Some(sub) = ident.pat() {
                    self.unsupported(sub.syntax().text_range(), "`@` patterns");
                }
                match ident.name() {
                    Some(name) => self.lower_ident_pat(name, ident, site),
                    None => Pat::Missing,
                }
            }
            ast::Pat::WildcardPat(_) => Pat::Wild,
            ast::Pat::ParenPat(paren) => return self.lower_pat(paren.pat(), pat.syntax(), site),
            ast::Pat::TuplePat(tuple) => {
                let (pats, rest) = self.lower_tuple_fields(tuple.fields(), site);
                Pat::Tuple { pats, rest }
            }
            ast::Pat::TupleStructPat(tuple) => {
                let strukt = self.resolve_struct(tuple.path(), range);
                let (pats, rest) = self.lower_tuple_fields(tuple.fields(), site);
                let Some(strukt) = strukt else {
                    return self.alloc_pat(Pat::Missing, range);
                };
                let s = &self.items.structs[strukt];
                if s.kind != StructKind::Tuple {
                    let message = format!(
                        "cannot find tuple struct or tuple variant `{}` in this scope",
                        s.name
                    );
                    self.error(range, message);
                    Pat::Missing
                } else {
                    Pat::TupleStruct { strukt, pats, rest }
                }
            }
            ast::Pat::RecordPat(record) => return self.lower_record_pat(record, site),
            ast::Pat::PathPat(path) => {
                let strukt = self.resolve_struct(path.path(), range);
                match strukt {
                    Some(strukt) if self.items.structs[strukt].kind == StructKind::Unit => {
                        Pat::Unit(strukt)
                    }
                    Some(strukt) => {
                        let s = &self.items.structs[strukt];
                        let what = if s.kind == StructKind::Tuple {
                            "tuple struct"
                        } else {
                            "struct"
                        };
                        let message = format!(
                            "expected unit struct, unit variant or constant, found {what} `{}`",
                            s.name
                        );
                        self.error(range, message);
                        Pat::Missing
                    }
                    None => Pat::Missing,
                }
            }
            ast::Pat::LiteralPat(_) | ast::Pat::RangePat(_) => {
                self.unsupported(range, "literal and range patterns");
                Pat::Missing
            }
            ast::Pat::RestPat(_) => {
                self.error(range, "`..` patterns are not allowed here");
                Pat::Missing
            }
            _ => {
                let what = match pat {
                    ast::Pat::SlicePat(_) => "slice patterns",
                    ast::Pat::RefPat(_) => "reference patterns",
                    ast::Pat::OrPat(_) => "or-patterns",
                    _ => "macros",
                };
                self.unsupported(range, what);
                Pat::Missing
            }
        };
        self.alloc_pat(hir, range)
    }

    /// `x`, `mut x`, or a unit struct's name, which is a pattern matching
    /// it rather than a new binding.
    fn lower_ident_pat(
        &mut self,
        name: ast::Name<'_>,
        ident: ast::IdentPat<'_>,
        site: PatSite,
    ) -> Pat {
        let range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let shadowed = self
            .items
            .struct_by_name(name)
            .filter(|(_, s)| s.kind != StructKind::Record);
        if let Some((id, s)) = shadowed {
            if s.kind == StructKind::Unit && ident.mut_token().is_none() {
                return Pat::Unit(id);
            }
            let site = match site {
                PatSite::Let => "let bindings",
                PatSite::Param => "function parameters",
            };
            let what = if s.kind == StructKind::Unit {
                "unit structs"
            } else {
                "tuple structs"
            };
            self.error(range, format!("{site} cannot shadow {what}"));
        }
        let binding = self.bindings.alloc(Binding {
            name,
            mutable: ident.mut_token().is_some(),
        });
        self.scopes.push((name, binding));
        Pat::Bind(binding)
    }

    /// The fields of a tuple or tuple struct pattern, and where `..` is.
    fn lower_tuple_fields(
        &mut self,
        fields: ast::AstChildren<'_, ast::Pat<'_>>,
        site: PatSite,
    ) -> (Box<[PatId]>, Option<u32>) {
        let mut pats = Vec::new();
        let mut rest = None;
        for field in fields {
            if let ast::Pat::RestPat(dots) = field {
                if rest.is_some() {
                    self.error(
                        dots.syntax().text_range(),
                        "`..` can only be used once per tuple pattern",
                    );
                }
                rest.get_or_insert(pats.len() as u32);
                continue;
            }
            let parent = field.syntax();
            pats.push(self.lower_pat(Some(field), parent, site));
        }
        (pats.into(), rest)
    }

    /// `S { a, b: x, .. }`
    fn lower_record_pat(&mut self, record: ast::RecordPat<'_>, site: PatSite) -> PatId {
        let range = record.syntax().text_range();
        let strukt = self.resolve_struct(record.path(), range);
        let mut fields = Vec::new();
        let mut name_ranges = Vec::new();
        let mut rest = false;
        if let Some(list) = record.record_pat_field_list() {
            rest = list.rest_pat().is_some();
            for field in list.fields() {
                check_attrs(field.attrs(), &mut self.diagnostics);
                let (name, name_range) = match (field.name_ref(), field.pat()) {
                    (Some(name), _) => (name.text(), name.syntax().text_range()),
                    // `S { a }` and `S { mut a }` bind the field `a`.
                    (None, Some(ast::Pat::IdentPat(ident))) => match ident.name() {
                        Some(name) => (name.text(), name.syntax().text_range()),
                        None => continue,
                    },
                    _ => continue,
                };
                let pat = self.lower_pat(field.pat(), field.syntax(), site);
                fields.push((Symbol::intern(name), pat));
                name_ranges.push((name_range, field.syntax().text_range()));
            }
        }
        let Some(strukt) = strukt else {
            return self.alloc_pat(Pat::Missing, range);
        };
        let id = self.alloc_pat(
            Pat::Record {
                strukt,
                fields: fields.into(),
                rest,
            },
            range,
        );
        self.source_map.pat_fields.insert(id, name_ranges.into());
        id
    }
}

/// The name of a path with one plain segment: `x`, `S`.
fn simple_path(path: ast::Path<'_>) -> Option<&str> {
    path.qualifier()
        .is_none()
        .then(|| path.segment())
        .flatten()
        .filter(|s| {
            s.colon2_token().is_none()
                && s.generic_arg_list().is_none()
                && s.parenthesized_arg_list().is_none()
        })
        .and_then(|s| s.name_ref())
        .filter(|n| n.ident_token().is_some())
        .map(|n| n.text())
}
