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
use crate::{Diagnostic, FunctionId, ItemTree};

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
            params.push(ctx.lower_pat(param.pat(), param.syntax()));
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
                let callee = self.lower_opt(call.expr(), syntax);
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
                match bin.op_kind() {
                    Some(op) => Expr::Binary { op, lhs, rhs },
                    None => Expr::Missing,
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
            ast::Expr::FieldExpr(_) => self.unsupported_expr(range, "field accesses"),
            ast::Expr::ForExpr(_) => self.unsupported_expr(range, "`for` loops"),
            ast::Expr::IndexExpr(_) => self.unsupported_expr(range, "index expressions"),
            ast::Expr::LetExpr(_) => self.unsupported_expr(range, "`let` conditions"),
            ast::Expr::MacroExpr(_) => self.unsupported_expr(range, "macros"),
            ast::Expr::MatchExpr(_) => self.unsupported_expr(range, "`match` expressions"),
            ast::Expr::MethodCallExpr(_) => self.unsupported_expr(range, "method calls"),
            ast::Expr::RangeExpr(_) => self.unsupported_expr(range, "ranges"),
            ast::Expr::RecordExpr(_) => self.unsupported_expr(range, "struct expressions"),
            ast::Expr::RefExpr(_) => self.unsupported_expr(range, "references"),
            ast::Expr::TryExpr(_) => self.unsupported_expr(range, "`?` expressions"),
            ast::Expr::UnderscoreExpr(_) => {
                self.unsupported_expr(range, "destructuring assignments")
            }
        };
        self.alloc_expr(hir, range)
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
                    let pat = self.lower_pat(let_stmt.pat(), let_stmt.syntax());
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
        let name = path
            .path()
            .filter(|p| p.qualifier().is_none())
            .and_then(|p| p.segment())
            .filter(|s| s.colon2_token().is_none() && s.generic_arg_list().is_none())
            .and_then(|s| s.name_ref())
            .filter(|n| n.ident_token().is_some());
        let Some(name) = name else {
            self.unsupported(range, "paths like this");
            return Res::Err;
        };
        let name = Symbol::intern(name.text());
        if let Some(&(_, binding)) = self.scopes.iter().rev().find(|(n, _)| *n == name) {
            return Res::Local(binding);
        }
        if let Some((function, _)) = self.items.function_by_name(name) {
            return Res::Function(function);
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
            Some(LiteralKind::String | LiteralKind::ByteString | LiteralKind::CString) => {
                self.unsupported_expr(range, "string literals")
            }
            None => Expr::Missing,
        }
    }

    fn lower_pat(&mut self, pat: Option<ast::Pat<'_>>, parent: SyntaxNode<'_>) -> PatId {
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
                    Some(name) => {
                        let name = Symbol::intern(name.text());
                        let binding = self.bindings.alloc(Binding {
                            name,
                            mutable: ident.mut_token().is_some(),
                        });
                        self.scopes.push((name, binding));
                        Pat::Bind(binding)
                    }
                    None => Pat::Missing,
                }
            }
            ast::Pat::WildcardPat(_) => Pat::Wild,
            ast::Pat::ParenPat(paren) => return self.lower_pat(paren.pat(), pat.syntax()),
            ast::Pat::LiteralPat(_) | ast::Pat::RangePat(_) => {
                self.unsupported(range, "literal and range patterns");
                Pat::Missing
            }
            _ => {
                let what = match pat {
                    ast::Pat::TuplePat(_) => "tuple patterns",
                    ast::Pat::RecordPat(_) | ast::Pat::TupleStructPat(_) => "struct patterns",
                    ast::Pat::PathPat(_) => "path patterns",
                    ast::Pat::SlicePat(_) => "slice patterns",
                    ast::Pat::RefPat(_) => "reference patterns",
                    ast::Pat::OrPat(_) => "or-patterns",
                    ast::Pat::RestPat(_) => "rest patterns",
                    _ => "macros",
                };
                self.unsupported(range, what);
                Pat::Missing
            }
        };
        self.alloc_pat(hir, range)
    }
}

/// Parses an integer literal: `1_000`, `0xff_u8`, `0b1010`.
fn parse_int(text: &str) -> Result<Literal, String> {
    let (radix, rest) = match text.get(..2) {
        Some("0x") => (16, &text[2..]),
        Some("0o") => (8, &text[2..]),
        Some("0b") => (2, &text[2..]),
        _ => (10, text),
    };
    let digits_end = rest
        .find(|c: char| !(c.is_digit(radix) || c == '_'))
        .unwrap_or(rest.len());
    let (digits, suffix) = rest.split_at(digits_end);
    let suffix = match suffix {
        "" => None,
        "f32" | "f64" if radix == 10 => {
            return Err("floating-point numbers are not supported yet".to_owned());
        }
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128"
        | "isize" => Some(Symbol::intern(suffix)),
        _ => return Err(format!("invalid suffix `{suffix}` for number literal")),
    };
    let mut value: u128 = 0;
    let mut any_digit = false;
    for c in digits.chars().filter(|&c| c != '_') {
        let digit = c.to_digit(radix).expect("digits were checked") as u128;
        value = value
            .checked_mul(radix as u128)
            .and_then(|v| v.checked_add(digit))
            .ok_or("integer literal is too large")?;
        any_digit = true;
    }
    if !any_digit {
        return Err("no valid digits found for number".to_owned());
    }
    Ok(Literal::Int { value, suffix })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_literals() {
        let int = |text| match parse_int(text) {
            Ok(Literal::Int { value, suffix }) => Ok((value, suffix.map(Symbol::as_str))),
            Ok(other) => panic!("{other:?}"),
            Err(message) => Err(message),
        };
        assert_eq!(int("0"), Ok((0, None)));
        assert_eq!(int("1_000"), Ok((1000, None)));
        assert_eq!(int("0xff_u8"), Ok((255, Some("u8"))));
        assert_eq!(int("0b1010i64"), Ok((10, Some("i64"))));
        assert_eq!(int("0o17"), Ok((15, None)));
        assert_eq!(
            int("340282366920938463463374607431768211455"),
            Ok((u128::MAX, None))
        );
        assert!(int("340282366920938463463374607431768211456").is_err());
        assert!(int("1u7").is_err());
        assert!(int("0x").is_err());
        assert!(int("1f32").is_err());
    }
}
