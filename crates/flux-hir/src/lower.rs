//! Lowers a function body from the typed AST to HIR.

use flux_arena::Arena;
use flux_intern::{Symbol, kw};
use flux_syntax::ast::{self, AstNode, LiteralKind};
use flux_syntax::{SyntaxNode, SyntaxTree, TextRange};

use flux_arena::ArenaMap;

use crate::body::{
    Binding, BindingId, Body, BodySourceMap, Expr, ExprId, Literal, LoopSource, Pat, PatId,
    PathGenerics, Res, SegmentArgs, Stmt,
};
use crate::def_map::{Def, Ns, ResolveError, Resolved};
use crate::item_tree::{
    GenericArgs, PathAnchor, lower_generic_args, lower_generic_path, lower_path,
};
use crate::item_tree::{check_attr, check_attrs, lower_type, unsupported};
use crate::literal::{parse_int, unescape_c_str};
use crate::{
    Bound, ConstId, Diagnostic, FunctionId, ImplTarget, ItemTree, ModuleId, Owner, Path, StructId,
    StructKind, TypeRef,
};

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

/// Lowers the value of constant `konst`, which must have one: a body
/// without parameters.
pub fn lower_const(tree: &SyntaxTree, items: &ItemTree, konst: ConstId) -> LoweredBody {
    let data = &items.consts[konst];
    let c = ast::Const::cast(tree.node(data.node)).expect("constants point at CONST nodes");
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, None);
    let root = ctx.lower_opt(c.body(), c.syntax());
    ctx.finish(Vec::new(), root)
}

/// Lowers the body of `function`, which must have one.
pub fn lower_body(tree: &SyntaxTree, items: &ItemTree, function: FunctionId) -> LoweredBody {
    let data = &items.functions[function];
    let node = tree.node(data.node);
    let f = ast::Fn::cast(node).expect("functions point at FN nodes");
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, Some(&data.generics));
    let mut params = Vec::new();
    if let Some(list) = f.param_list() {
        if let Some(param) = list.self_param().filter(|_| data.has_self) {
            let binding = ctx.bindings.alloc(Binding {
                name: kw::SelfLower,
                mutable: param.amp_token().is_none() && param.mut_token().is_some(),
            });
            ctx.scopes.push((kw::SelfLower, binding));
            let range = param.syntax().text_range();
            params.push(ctx.alloc_pat(Pat::Bind(binding), range));
        }
        for param in list.params() {
            params.push(ctx.lower_pat(param.pat(), param.syntax(), PatSite::Param));
        }
        // Each name may be bound by one parameter only. Most parameters
        // are names, compared without allocating.
        let name = |ctx: &Ctx<'_>, p: PatId| match ctx.pats[p] {
            Pat::Bind(b) => Some(ctx.bindings[b].name),
            _ => None,
        };
        let simple = params
            .iter()
            .all(|&p| matches!(ctx.pats[p], Pat::Bind(_) | Pat::Wild | Pat::Missing));
        if simple {
            for i in 1..params.len() {
                let Some(n) = name(&ctx, params[i]) else {
                    continue;
                };
                if params[..i].iter().any(|&q| name(&ctx, q) == Some(n)) {
                    let range = ctx.source_map.pats.get(params[i]).copied();
                    ctx.error(
                        range.unwrap_or_default(),
                        format!("identifier `{n}` is bound more than once in this parameter list"),
                    );
                }
            }
        } else if params.len() > 1 {
            let mut binds = Vec::new();
            let mut stack: Vec<PatId> = params.iter().rev().copied().collect();
            while let Some(p) = stack.pop() {
                if let Pat::Bind(b) = ctx.pats[p] {
                    binds.push((ctx.bindings[b].name, p));
                }
                let first = stack.len();
                stack.extend(ctx.pats[p].children());
                stack[first..].reverse();
            }
            for i in 1..binds.len() {
                let (name, p) = binds[i];
                if binds[..i].iter().any(|&(n, _)| n == name) {
                    let range = ctx.source_map.pats.get(p).copied().unwrap_or_default();
                    ctx.error(
                        range,
                        format!(
                            "identifier `{name}` is bound more than once in this parameter list"
                        ),
                    );
                }
            }
        }
    }
    let root = match f.body() {
        Some(block) => ctx.lower_block(block),
        None => ctx.alloc_expr(Expr::Missing, f.syntax().text_range()),
    };
    ctx.finish(params, root)
}

struct Ctx<'a> {
    items: &'a ItemTree,
    /// Lowering a function in an `impl` block, where `Self` means its
    /// type: `self_target`, unless that did not resolve.
    in_impl: bool,
    self_target: Option<ImplTarget>,
    /// In a trait or a trait's `impl` block, where `Self` may be any
    /// type, and `Self::f` is left to type checking.
    self_is_any: bool,
    /// The module the function is in, where its names are resolved.
    module: ModuleId,
    /// The generic parameters in scope: the `impl` block's, then the
    /// function's.
    generics: Vec<Symbol>,
    exprs: Arena<Expr>,
    pats: Arena<Pat>,
    bindings: Arena<Binding>,
    types: Arena<crate::TypeRef>,
    source_map: BodySourceMap,
    generic_args: ArenaMap<Expr, PathGenerics>,
    /// Bindings in scope, innermost last; a block truncates it on exit.
    scopes: Vec<(Symbol, BindingId)>,
    /// The enclosing loops, innermost last.
    loops: Vec<ExprId>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Ctx<'a> {
    /// Lowering an item in `module`, in the `impl` block or trait
    /// `owner`, with its own generic parameters if it has any.
    fn new(
        items: &'a ItemTree,
        module: ModuleId,
        owner: Option<Owner>,
        own: Option<&crate::Generics>,
    ) -> Ctx<'a> {
        let generics = items
            .owner_generics(owner)
            .into_iter()
            .chain(own)
            .flat_map(|g| g.params.iter().map(|p| p.name))
            .collect();
        let imp = owner.and_then(Owner::as_impl).map(|i| &items.impls[i]);
        Ctx {
            items,
            module,
            generics,
            in_impl: owner.is_some(),
            // A trait's block for a struct may use its constructor as
            // `Self`.
            self_target: imp.and_then(|imp| {
                imp.target.or_else(|| match &imp.self_ty {
                    TypeRef::Path(path) => match items.resolve(imp.module, path, Ns::Types) {
                        Ok(Resolved::Def(Def::Struct(s))) => Some(ImplTarget::Struct(s)),
                        _ => None,
                    },
                    _ => None,
                })
            }),
            self_is_any: owner.is_some_and(|o| match o {
                Owner::Trait(_) => true,
                Owner::Impl(i) => items.impls[i].trait_ref.is_some(),
            }),
            exprs: Arena::new(),
            pats: Arena::new(),
            bindings: Arena::new(),
            types: Arena::new(),
            source_map: BodySourceMap::default(),
            generic_args: ArenaMap::new(),
            scopes: Vec::new(),
            loops: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn finish(self, params: Vec<PatId>, root: ExprId) -> LoweredBody {
        LoweredBody {
            body: Body {
                exprs: self.exprs,
                pats: self.pats,
                bindings: self.bindings,
                types: self.types,
                params: params.into(),
                generic_args: self.generic_args,
                root,
            },
            source_map: self.source_map,
            diagnostics: self.diagnostics,
        }
    }
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
            ast::Expr::PathExpr(path) => return self.lower_path_expr(path, None),
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
                    // A callee's path is resolved knowing it is called, for
                    // rustc's messages.
                    Some(ast::Expr::PathExpr(path)) => {
                        self.check_attrs(path.syntax());
                        self.lower_path_expr(path, Some(range))
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
                let generics = call.generic_arg_list().map(|list| {
                    let args = lower_generic_args(list, &mut self.diagnostics);
                    args.reject_bindings(&mut self.diagnostics);
                    args
                });
                let receiver = self.lower_opt(call.receiver(), syntax);
                let args = match call.arg_list() {
                    Some(list) => list.args().map(|e| self.lower_expr(e)).collect(),
                    None => Box::default(),
                };
                match call.name_ref() {
                    Some(name) => {
                        let method = Symbol::intern(name.text());
                        let call = Expr::MethodCall {
                            receiver,
                            method,
                            args,
                        };
                        let id = self.alloc_expr(call, range);
                        let name_range = name.syntax().text_range();
                        self.source_map
                            .expr_fields
                            .insert(id, Box::new([name_range]));
                        if let Some(generics) = generics {
                            let item = self.segment_args(name_range, &generics);
                            let generics = PathGenerics {
                                item: Some(item),
                                ..PathGenerics::default()
                            };
                            self.generic_args.insert(id, generics);
                        }
                        return id;
                    }
                    None => Expr::Missing,
                }
            }
            ast::Expr::RangeExpr(_) => self.unsupported_expr(range, "ranges"),
            ast::Expr::RecordExpr(record) => return self.lower_record(record),
            ast::Expr::RefExpr(r) => {
                let expr = self.lower_opt(r.expr(), syntax);
                Expr::Ref {
                    mutable: r.mut_token().is_some(),
                    expr,
                }
            }
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
        let path = record
            .path()
            .and_then(|p| lower_generic_path(p, &mut self.diagnostics));
        if let Some(path) = &path {
            path.reject_bindings(&mut self.diagnostics);
        }
        let path_range = record.path().map_or(range, |p| p.syntax().text_range());
        let strukt = path
            .as_ref()
            .and_then(|path| self.resolve_struct_path(path, path_range));
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
        if let Some(path) = path {
            let self_type = path.as_name() == Some(kw::SelfUpper);
            let item = (!self_type).then_some(path.segments.len() - 1);
            let mut generics = self.path_generics(&path, path_range, item, None, Res::Err);
            if self_type {
                generics.get_or_insert_default().self_type = true;
            }
            if let Some(generics) = generics {
                self.generic_args.insert(id, generics);
            }
        }
        id
    }

    /// The struct a path in a pattern names.
    fn resolve_struct(
        &mut self,
        path: Option<ast::Path<'_>>,
        range: TextRange,
    ) -> Option<StructId> {
        let range = path.map_or(range, |p| p.syntax().text_range());
        let path = path.and_then(|p| lower_path(p, &mut self.diagnostics))?;
        self.resolve_struct_path(&path, range)
    }

    /// The struct a path in a struct expression or pattern names.
    fn resolve_struct_path(&mut self, path: &Path, range: TextRange) -> Option<StructId> {
        if path.anchor == PathAnchor::Plain && self.generics.contains(&path.segments[0]) {
            let message = match path.segments.len() {
                1 => format!(
                    "expected struct, variant or union type, found type parameter `{}`",
                    path.segments[0]
                ),
                _ => "ambiguous associated type".to_owned(),
            };
            self.error(range, message);
            return None;
        }
        if path.as_name() == Some(kw::SelfUpper) {
            if !self.in_impl {
                self.error(
                    range,
                    "cannot find struct, variant or union type `Self` in this scope",
                );
            }
            return self.self_struct();
        }
        match self.reported(
            self.items.resolve(self.module, path, Ns::Types),
            path,
            range,
        ) {
            Ok(Resolved::Def(Def::Struct(strukt))) => Some(strukt),
            Ok(Resolved::Def(Def::Module(m))) => {
                let last = path.segments.len() - 1;
                let message = format!(
                    "expected struct, variant or union type, found module `{}`",
                    self.items.modules[m].name
                );
                self.error(path.range(last, range), message);
                None
            }
            Err(e @ ResolveError::Failed { .. }) => {
                let ResolveError::Failed { message, .. } = &e else {
                    unreachable!()
                };
                self.error(e.range(path, range), message.clone());
                None
            }
            result => {
                let segment = match &result {
                    Err(e) => e.segment(),
                    Ok(_) => path.segments.len() - 1,
                };
                let place = match result {
                    Err(ResolveError::NotFound {
                        module: Some(module),
                        ..
                    }) => self.items.in_module(module),
                    _ => "this scope".to_owned(),
                };
                let name = path.segments[segment];
                self.error(
                    path.range(segment, range),
                    format!("cannot find struct, variant or union type `{name}` in {place}"),
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

    /// A path expression, called if `call` is the call's range.
    fn lower_path_expr(&mut self, path: ast::PathExpr<'_>, call: Option<TextRange>) -> ExprId {
        let range = path.syntax().text_range();
        let segment = path
            .path()
            .filter(|p| p.qualifier().is_some())
            .and_then(|p| p.segment()?.name_ref())
            .map(|n| n.syntax().text_range());
        let (res, generics) = self.resolve_path(path, call);
        let id = self.alloc_expr(Expr::Path(res), range);
        if let Some(generics) = generics {
            self.generic_args.insert(id, generics);
        }
        if let Some(segment) = segment {
            self.source_map.segments.insert(id, segment);
        }
        id
    }

    /// A path expression's meaning and generic arguments.
    fn resolve_path(
        &mut self,
        path: ast::PathExpr<'_>,
        call: Option<TextRange>,
    ) -> (Res, Option<PathGenerics>) {
        let range = path.syntax().text_range();
        let Some(path) = path
            .path()
            .and_then(|p| lower_generic_path(p, &mut self.diagnostics))
        else {
            return (Res::Err, None);
        };
        path.reject_bindings(&mut self.diagnostics);
        let res = self.resolve_value(&path, range, call);
        let last = path.segments.len() - 1;
        let (item, ty) = match res {
            Res::Assoc { .. } if path.qself.is_none() && last > 0 => (Some(last), Some(last - 1)),
            Res::Assoc { .. } => (Some(last), None),
            Res::Function(f) if self.items.functions[f].owner.is_some() && last > 0 => {
                (Some(last), Some(last - 1))
            }
            Res::Function(_) | Res::Struct(_) => (Some(last), None),
            Res::Const(c) if self.items.consts[c].owner.is_some() && last > 0 => {
                (None, Some(last - 1))
            }
            Res::Local(_) | Res::Const(_) => (None, None),
            Res::Err => return (res, None),
        };
        // `Self` has no generic arguments of its own.
        let is_self = |i: usize| {
            path.anchor == PathAnchor::Plain && i == 0 && path.segments[0] == kw::SelfUpper
        };
        let self_type = match res {
            Res::Struct(_) => item.is_some_and(is_self),
            _ => ty.is_some_and(is_self),
        };
        let ty = ty.filter(|&i| !is_self(i));
        let item = item.filter(|&i| !is_self(i));
        // A constant has no generic parameters.
        let last_args = path.args(last).filter(|_| matches!(res, Res::Const(_)));
        if let Some(args) = last_args {
            let got = args.types.len();
            let (s, verb) = if got == 1 { ("", "was") } else { ("s", "were") };
            self.error(
                path.range(last, range),
                format!("constant takes 0 generic arguments but {got} generic argument{s} {verb} supplied"),
            );
        }
        let mut generics = self.path_generics(&path, range, item, ty, res);
        if self_type {
            generics.get_or_insert_default().self_type = true;
        }
        // `<T as Tr<A>>::f`: the trait's arguments.
        if let Res::Assoc {
            trait_: Some(_), ..
        } = res
            && let Some(trait_path) = path.qself.as_ref().and_then(|q| q.trait_.as_ref())
        {
            let last = trait_path.segments.len().saturating_sub(1);
            for (i, args) in trait_path.args.iter().enumerate() {
                let Some(args) = args else {
                    continue;
                };
                if i == last {
                    let segment = trait_path.range(i, range);
                    generics.get_or_insert_default().ty = Some(self.segment_args(segment, args));
                } else {
                    let name = trait_path.segments[i];
                    let at = args.ranges.first().copied().unwrap_or(args.range);
                    self.error(
                        at,
                        format!("type arguments are not allowed on module `{name}`"),
                    );
                }
            }
        }
        (res, generics)
    }

    /// The generic arguments of `path`, which resolved to `res`, after its
    /// `item` and `ty` segments, reporting any after other segments.
    fn path_generics(
        &mut self,
        path: &Path,
        range: TextRange,
        item: Option<usize>,
        ty: Option<usize>,
        res: Res,
    ) -> Option<PathGenerics> {
        if path.args.is_empty() {
            return None;
        }
        let mut generics = PathGenerics::default();
        for (i, args) in path.args.iter().enumerate() {
            let Some(args) = args else {
                continue;
            };
            let segment = path.range(i, range);
            if Some(i) == item {
                generics.item = Some(self.segment_args(segment, args));
            } else if Some(i) == ty {
                generics.ty = Some(self.segment_args(segment, args));
            } else {
                let name = path.segments[i];
                let last = i + 1 == path.segments.len();
                let what = match res {
                    // Reported with the constant's arguments.
                    Res::Const(_) if last => continue,
                    Res::Struct(_) if last && name == kw::SelfUpper => {
                        "self constructor".to_owned()
                    }
                    _ if name == kw::SelfUpper => "self type".to_owned(),
                    Res::Local(_) => "local variable".to_owned(),
                    _ if crate::is_primitive(name) && i + 2 == path.segments.len() => {
                        format!("builtin type `{name}`")
                    }
                    _ => format!("module `{name}`"),
                };
                let at = args.ranges.first().copied().unwrap_or(args.range);
                self.error(at, format!("type arguments are not allowed on {what}"));
            }
        }
        (generics.item.is_some() || generics.ty.is_some()).then_some(generics)
    }

    /// Generic arguments in an expression, as types of the body.
    fn segment_args(&mut self, segment: TextRange, args: &GenericArgs) -> SegmentArgs {
        let types = args
            .types
            .iter()
            .zip(&args.ranges)
            .map(|(ty, &range)| {
                let id = self.types.alloc(ty.clone());
                self.source_map.types.insert(id, range);
                id
            })
            .collect();
        SegmentArgs {
            segment,
            range: args.range,
            types,
        }
    }

    /// A path in the value namespace: a local, a function, an associated
    /// function, or a tuple or unit struct. `call` is the call's range if
    /// it is called, which changes rustc's messages.
    fn resolve_value(&mut self, path: &Path, range: TextRange, call: Option<TextRange>) -> Res {
        if let Some(name) = path.as_name() {
            if let Some(&(_, binding)) = self.scopes.iter().rev().find(|(n, _)| *n == name) {
                return Res::Local(binding);
            }
            if name == kw::SelfLower {
                self.error(range, "cannot find value `self` in this scope");
                return Res::Err;
            }
            if name == kw::SelfUpper {
                let Some(strukt) = self.self_type(range) else {
                    return Res::Err;
                };
                if self.items.structs[strukt].kind == StructKind::Record {
                    self.error(
                        range,
                        "the `Self` constructor can only be used with tuple or unit structs",
                    );
                    return Res::Err;
                }
                return Res::Struct(strukt);
            }
        }
        if path.qself.is_some() {
            return self.resolve_qualified(path, range);
        }
        // `T::f`, and `Self::f` in a trait or a trait's block: an item of
        // a trait, or of `Self`'s type, that type checking finds.
        if path.anchor == PathAnchor::Plain
            && path.segments.len() > 1
            && (self.generics.contains(&path.segments[0])
                || (path.segments[0] == kw::SelfUpper && self.self_is_any))
        {
            if path.segments.len() > 2 {
                self.unsupported(path.range(1, range), "associated types");
                return Res::Err;
            }
            return self.type_relative(path, 1, range);
        }
        // `Self::f`: an associated function of this block's type.
        let resolved = match (path.anchor, path.segments.first()) {
            (PathAnchor::Plain, Some(&first))
                if first == kw::SelfUpper && path.segments.len() > 1 =>
            {
                match self.self_target {
                    Some(target) => {
                        self.items
                            .resolve_assoc(self.module, target, path, 1, Ns::Values)
                    }
                    None => {
                        if !self.in_impl {
                            self.error(
                                path.range(0, range),
                                "cannot find type `Self` in this scope",
                            );
                        }
                        return Res::Err;
                    }
                }
            }
            _ => self.items.resolve(self.module, path, Ns::Values),
        };
        match self.reported(resolved, path, range) {
            Ok(Resolved::TypeRelative(index)) => self.type_relative(path, index, range),
            Ok(Resolved::TraitItem(t, _)) => Res::Assoc {
                ty: None,
                trait_: Some(t),
                name: path.segments[path.segments.len() - 1],
            },
            Ok(Resolved::Def(Def::Function(f)) | Resolved::Assoc(f)) => Res::Function(f),
            Ok(Resolved::Def(Def::Const(c)) | Resolved::AssocConst(c)) => Res::Const(c),
            Ok(Resolved::Def(Def::Struct(strukt))) => {
                let s = &self.items.structs[strukt];
                let private = s.kind == StructKind::Tuple
                    && s.fields
                        .iter()
                        .any(|f| !self.items.is_visible(f.vis, self.module));
                if private {
                    let last = path.segments.len() - 1;
                    let message = format!("tuple struct constructor `{}` is private", s.name);
                    self.error(path.range(last, range), message);
                }
                Res::Struct(strukt)
            }
            Ok(Resolved::SelfType) => self.resolve_value(&Path::single(kw::SelfUpper), range, call),
            Ok(_) => {
                self.unsupported(range, "paths like this");
                Res::Err
            }
            Err(e @ ResolveError::Failed { .. }) => {
                let ResolveError::Failed { message, .. } = &e else {
                    unreachable!()
                };
                self.error(e.range(path, range), message.clone());
                Res::Err
            }
            Err(ResolveError::Private { .. } | ResolveError::Reported) => Res::Err,
            Err(ResolveError::NotFound { segment, module }) => {
                // A struct with named fields is only in the type namespace.
                let is_struct = matches!(
                    self.items.resolve(self.module, path, Ns::Types),
                    Ok(Resolved::Def(Def::Struct(_)))
                );
                let name = path.segments[segment];
                let place = match module {
                    // rustc's wording for `self::f`.
                    Some(_) if path.anchor == PathAnchor::SelfModule && segment == 0 => {
                        "module `self`".to_owned()
                    }
                    Some(module) => self.items.in_module(module),
                    None => "this scope".to_owned(),
                };
                let (message, range) = match call {
                    // rustc reports the whole call for a struct called like
                    // a function.
                    Some(call) if is_struct => (
                        format!(
                            "cannot find function, tuple struct or tuple variant `{name}` in {place}"
                        ),
                        call,
                    ),
                    Some(_) => (
                        format!("cannot find function `{name}` in {place}"),
                        path.range(segment, range),
                    ),
                    None => (
                        format!("cannot find value `{name}` in {place}"),
                        path.range(segment, range),
                    ),
                };
                self.error(range, message);
                Res::Err
            }
        }
    }

    /// `T::f`, where the segments before `index` name a type `T`, and
    /// segment `index` is the last.
    fn type_relative(&mut self, path: &Path, index: usize, range: TextRange) -> Res {
        let prefix = Path {
            segments: path.segments[..index].into(),
            ranges: path.ranges[..index.min(path.ranges.len())].into(),
            args: path.args[..index.min(path.args.len())].into(),
            ..path.clone()
        };
        let ty_range = TextRange::new(
            range.start(),
            path.range(index - 1, range).end().max(range.start()),
        );
        let ty = self.types.alloc(TypeRef::Path(prefix));
        self.source_map.types.insert(ty, ty_range);
        Res::Assoc {
            ty: Some(ty),
            trait_: None,
            name: path.segments[index],
        }
    }

    /// `<T as Tr>::f` or `<T>::f`.
    fn resolve_qualified(&mut self, path: &Path, range: TextRange) -> Res {
        let qself = path.qself.as_ref().expect("a qualified path");
        if path.segments.len() != 1 {
            self.unsupported(path.range(0, range), "associated types");
            return Res::Err;
        }
        let name = path.segments[0];
        let trait_ = match &qself.trait_ {
            None => None,
            Some(trait_path) => {
                let bound = Bound {
                    path: trait_path.clone(),
                    range: qself.range,
                };
                let (t, e) = self.items.resolve_trait(self.module, &bound);
                if let Some((at, message)) = e {
                    self.error(at, message);
                }
                let Some(t) = t else {
                    return Res::Err;
                };
                let data = &self.items.traits[t];
                if data.unsupported.contains(&name) {
                    return Res::Err;
                }
                if data.function(self.items, name).is_none() {
                    let message = format!(
                        "cannot find method or associated constant `{name}` in trait `{}`",
                        data.name
                    );
                    self.error(path.range(0, range), message);
                    return Res::Err;
                }
                Some(t)
            }
        };
        let ty = self.types.alloc(qself.ty.clone());
        self.source_map.types.insert(ty, qself.ty_range);
        Res::Assoc {
            ty: Some(ty),
            trait_,
            name,
        }
    }

    /// A tuple struct's constructor is private wherever any of its fields
    /// is.
    fn check_constructor_privacy(
        &mut self,
        strukt: StructId,
        path: Option<ast::Path<'_>>,
        range: TextRange,
    ) {
        let s = &self.items.structs[strukt];
        if s.kind == StructKind::Tuple
            && s.fields
                .iter()
                .any(|f| !self.items.is_visible(f.vis, self.module))
        {
            let message = format!("tuple struct constructor `{}` is private", s.name);
            let range = path
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .map_or(range, |n| n.syntax().text_range());
            self.error(range, message);
        }
    }

    /// Reports a private item in `path` and carries on with it, as rustc
    /// does.
    fn reported(
        &mut self,
        result: Result<Resolved, ResolveError>,
        path: &Path,
        range: TextRange,
    ) -> Result<Resolved, ResolveError> {
        match result {
            Err(e @ ResolveError::Private { .. }) => {
                let ResolveError::Private {
                    message, resolved, ..
                } = &e
                else {
                    unreachable!()
                };
                self.error(e.range(path, range), message.clone());
                Ok(*resolved)
            }
            result => result,
        }
    }

    /// The struct `Self` names here.
    fn self_type(&mut self, range: TextRange) -> Option<StructId> {
        if !self.in_impl {
            self.error(range, "cannot find type `Self` in this scope");
        }
        self.self_struct()
    }

    fn self_struct(&self) -> Option<StructId> {
        match self.self_target {
            Some(ImplTarget::Struct(strukt)) => Some(strukt),
            _ => None,
        }
    }

    /// A name in the value namespace.
    fn resolve_name(&mut self, name: Symbol, range: TextRange) -> Res {
        self.resolve_value(&Path::single(name), range, None)
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
                if let Some(strukt) = strukt {
                    self.check_constructor_privacy(strukt, tuple.path(), range);
                }
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
        let shadowed = match self.items.lookup(self.module, name, Ns::Values) {
            Some(Resolved::Def(Def::Struct(id))) => Some((id, &self.items.structs[id])),
            // A constant's name is a pattern matching only its value.
            Some(Resolved::Def(Def::Const(_))) => {
                self.unsupported(range, "constant patterns");
                return Pat::Missing;
            }
            _ => None,
        };
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
