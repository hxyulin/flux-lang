//! Lowers a function body from the typed AST to HIR.

use flux_arena::Arena;
use flux_intern::{Symbol, kw};
use flux_syntax::ast::{self, AstNode, BinaryOp, CmpOp, LiteralKind, LogicOp};
use flux_syntax::{SyntaxNode, SyntaxTree, TextRange};

use flux_arena::ArenaMap;

use crate::body::{
    Binding, BindingId, Body, BodySourceMap, Expr, ExprId, Literal, LoopSource, MatchArm, Pat,
    PatId, PathGenerics, Res, SegmentArgs, Stmt,
};
use crate::def_map::{Def, Ns, ResolveError, Resolved};
use crate::item_tree::{GenericArgs, PathAnchor, lower_generic_args, lower_generic_path};
use crate::item_tree::{check_attr, check_attrs, lower_type, unsupported};
use crate::literal::{parse_int, unescape_c_str};
use crate::{
    Bound, ConstId, Derive, Diagnostic, FunctionId, ImplTarget, ItemTree, ModuleId, Owner, Path,
    StructId, StructKind, TypeRef,
};

/// Where a pattern binds, for "cannot shadow" messages.
#[derive(Clone, Copy)]
enum PatSite {
    Let,
    Param,
    Match,
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
    if let Some((strukt, _)) = data.discriminant {
        // An enum's discriminant, in the enum's scope: `Self` is the
        // enum, which is not generic (reported otherwise).
        let v = ast::Variant::cast(tree.node(data.node)).expect("discriminants point at variants");
        let mut ctx = Ctx::new(items, data.loc.module, None, None);
        ctx.in_impl = true;
        ctx.self_target = Some(ImplTarget::Struct(strukt));
        let root = ctx.lower_opt(v.expr(), v.syntax());
        return ctx.finish(Vec::new(), root);
    }
    let c = ast::Const::cast(tree.node(data.node)).expect("constants point at CONST nodes");
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, None);
    let root = ctx.lower_opt(c.body(), c.syntax());
    ctx.finish(Vec::new(), root)
}

/// Lowers the body of `function`, which must have one.
pub fn lower_body(tree: &SyntaxTree, items: &ItemTree, function: FunctionId) -> LoweredBody {
    let data = &items.functions[function];
    if let Some((strukt, derive)) = data.derived {
        return derived_body(items, function, strukt, derive);
    }
    let node = tree.node(data.node);
    let f = ast::Fn::cast(node).expect("functions point at FN nodes");
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, Some(&data.generics));
    let mut params = Vec::new();
    if let Some(list) = f.param_list() {
        if let Some(param) = list.self_param().filter(|_| data.has_self) {
            let binding = ctx.bindings.alloc(Binding {
                name: kw::SelfLower,
                mutable: param.amp_token().is_none() && param.mut_token().is_some(),
                by_ref: None,
            });
            ctx.scopes.push((kw::SelfLower, binding));
            let range = param.syntax().text_range();
            params.push(ctx.alloc_pat(Pat::Bind { binding, sub: None }, range));
        }
        for param in list.params() {
            if let Some(pat @ ast::Pat::OrPat(_)) = param.pat() {
                ctx.error(
                    pat.syntax().text_range(),
                    "function parameters require top-level or-patterns in parentheses",
                );
            }
            params.push(ctx.lower_top_pat(param.pat(), param.syntax(), PatSite::Param));
        }
        // Each name may be bound by one parameter only. Most parameters
        // are names, compared without allocating.
        let name = |ctx: &Ctx<'_>, p: PatId| match ctx.pats[p] {
            Pat::Bind { binding, sub: None } => Some(ctx.bindings[binding].name),
            _ => None,
        };
        let simple = params.iter().all(|&p| {
            matches!(
                ctx.pats[p],
                Pat::Bind { sub: None, .. } | Pat::Wild | Pat::Missing
            )
        });
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
                if let Pat::Bind { binding, .. } = ctx.pats[p] {
                    binds.push((ctx.bindings[binding].name, p));
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

/// The body of the method `#[derive]` makes for `strukt`, from its
/// fields: `S { a: Clone::clone(&self.a), .. }` for `Clone`,
/// `self.a == other.a && ..` for `PartialEq`, and a call of
/// `Eq::assert_receiver_is_total_eq(&self.a)` per field for `Eq`. Each
/// field's part is at the field, where rustc reports one that does not
/// implement the trait.
fn derived_body(
    items: &ItemTree,
    function: FunctionId,
    strukt: StructId,
    derive: Derive,
) -> LoweredBody {
    let data = &items.functions[function];
    let s = &items.structs[strukt];
    if s.is_enum() {
        return derived_enum_body(items, function, strukt, derive);
    }
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, Some(&data.generics));
    let mut params = Vec::new();
    let mut bindings = Vec::new();
    for name in [kw::SelfLower, Symbol::intern("other")]
        .into_iter()
        .take(data.params.len())
    {
        let binding = ctx.bindings.alloc(Binding {
            name,
            mutable: false,
            by_ref: None,
        });
        params.push(ctx.alloc_pat(Pat::Bind { binding, sub: None }, data.def_range));
        bindings.push(binding);
    }
    // `binding.field`, at the field.
    let field_of = |ctx: &mut Ctx<'_>, binding: BindingId, field: &crate::Field| {
        let at = field.def_range;
        let base = ctx.alloc_expr(Expr::Path(Res::Local(binding)), at);
        let place = ctx.alloc_expr(
            Expr::Field {
                expr: base,
                name: field.name,
            },
            at,
        );
        ctx.source_map
            .expr_fields
            .insert(place, Box::new([field.name_range]));
        place
    };
    // `Tr::method(&self.field)`, for the lang trait `lang`.
    let call = |ctx: &mut Ctx<'_>, lang: &str, method: &str, field: &crate::Field| {
        let at = field.def_range;
        let place = field_of(ctx, bindings[0], field);
        let arg = ctx.alloc_expr(
            Expr::Ref {
                mutable: false,
                expr: place,
            },
            at,
        );
        let res = match items.lang_traits.get(&Symbol::intern(lang)) {
            Some(&t) => Res::Assoc {
                ty: None,
                trait_: Some(t),
                name: Symbol::intern(method),
            },
            None => Res::Err,
        };
        let callee = ctx.alloc_expr(Expr::Path(res), at);
        ctx.source_map.segments.insert(callee, at);
        ctx.alloc_expr(
            Expr::Call {
                callee,
                args: Box::new([arg]),
            },
            at,
        )
    };
    let whole = s.name_range;
    let (stmts, tail) = match derive {
        Derive::Clone => {
            let fields: Box<[(Symbol, ExprId)]> = s
                .fields
                .iter()
                .map(|f| (f.name, call(&mut ctx, "clone", "clone", f)))
                .collect();
            let record = ctx.alloc_expr(
                Expr::Record {
                    strukt,
                    variant: 0,
                    fields,
                    spread: None,
                },
                whole,
            );
            let names: Box<[TextRange]> = s.fields.iter().map(|f| f.name_range).collect();
            ctx.source_map.expr_fields.insert(record, names);
            ctx.source_map.paths.insert(record, whole);
            (Vec::new(), Some(record))
        }
        Derive::PartialEq => {
            let mut all: Option<ExprId> = None;
            for f in s.fields.iter() {
                let lhs = field_of(&mut ctx, bindings[0], f);
                let rhs = field_of(&mut ctx, bindings[1], f);
                let eq = ctx.alloc_expr(
                    Expr::Binary {
                        op: BinaryOp::Cmp(CmpOp::Eq),
                        lhs,
                        rhs,
                    },
                    f.def_range,
                );
                ctx.source_map.ops.insert(eq, f.def_range);
                all = Some(match all {
                    None => eq,
                    Some(prev) => {
                        let and = ctx.alloc_expr(
                            Expr::Binary {
                                op: BinaryOp::Logic(LogicOp::And),
                                lhs: prev,
                                rhs: eq,
                            },
                            f.def_range,
                        );
                        ctx.source_map.ops.insert(and, f.def_range);
                        and
                    }
                });
            }
            let tail =
                all.unwrap_or_else(|| ctx.alloc_expr(Expr::Literal(Literal::Bool(true)), whole));
            (Vec::new(), Some(tail))
        }
        Derive::Eq => {
            let stmts = s
                .fields
                .iter()
                .map(|f| Stmt::Expr {
                    expr: call(&mut ctx, "total_eq", "assert_receiver_is_total_eq", f),
                    has_semicolon: true,
                })
                .collect();
            (stmts, None)
        }
        Derive::Copy => {
            // `{ assert_param_is_clone(&self.a); ..; *self }`
            let path = Path {
                anchor: PathAnchor::Global,
                qself: None,
                segments: ["core", "clone", "assert_param_is_clone"]
                    .map(Symbol::intern)
                    .into(),
                ranges: Box::default(),
                anchor_ranges: Box::default(),
                args: Box::default(),
            };
            let res = match items.resolve(data.loc.module, &path, Ns::Values) {
                Ok(Resolved::Def(Def::Function(f))) => Res::Function(f),
                _ => Res::Err,
            };
            let mut stmts = Vec::new();
            for f in s.fields.iter() {
                let at = f.def_range;
                let place = field_of(&mut ctx, bindings[0], f);
                let arg = ctx.alloc_expr(
                    Expr::Ref {
                        mutable: false,
                        expr: place,
                    },
                    at,
                );
                let callee = ctx.alloc_expr(Expr::Path(res), at);
                let call = ctx.alloc_expr(
                    Expr::Call {
                        callee,
                        args: Box::new([arg]),
                    },
                    at,
                );
                stmts.push(Stmt::Expr {
                    expr: call,
                    has_semicolon: true,
                });
            }
            let this = ctx.alloc_expr(Expr::Path(Res::Local(bindings[0])), whole);
            let copy = ctx.alloc_expr(
                Expr::Unary {
                    op: ast::UnaryOp::Deref,
                    operand: this,
                },
                whole,
            );
            (stmts, Some(copy))
        }
    };
    let root = ctx.alloc_expr(
        Expr::Block {
            stmts: stmts.into(),
            tail,
            is_unsafe: false,
        },
        whole,
    );
    ctx.finish(params, root)
}

/// The body of the method `#[derive]` makes for enum `strukt`: a `match`
/// on its variants, with a binding for each field. `Clone` builds the
/// variant from the fields' clones, `PartialEq` compares the fields of
/// the same variants (`E::A(a) => match other { E::A(b) => *a == *b, _
/// => false }`),
/// `Eq` checks each field's type, and `Clone` for an enum that derives
/// `Copy` too checks its fields are `Clone`, then copies.
fn derived_enum_body(
    items: &ItemTree,
    function: FunctionId,
    strukt: StructId,
    derive: Derive,
) -> LoweredBody {
    let data = &items.functions[function];
    let s = &items.structs[strukt];
    let variants = s.variants.as_deref().unwrap_or_default();
    let mut ctx = Ctx::new(items, data.loc.module, data.owner, Some(&data.generics));
    let whole = s.name_range;
    let mut params = Vec::new();
    let mut receivers = Vec::new();
    for name in [kw::SelfLower, Symbol::intern("other")]
        .into_iter()
        .take(data.params.len())
    {
        let binding = ctx.bindings.alloc(Binding {
            name,
            mutable: false,
            by_ref: None,
        });
        params.push(ctx.alloc_pat(Pat::Bind { binding, sub: None }, data.def_range));
        receivers.push(binding);
    }
    let local = |ctx: &mut Ctx<'_>, binding: BindingId, at: TextRange| {
        ctx.alloc_expr(Expr::Path(Res::Local(binding)), at)
    };
    // `Tr::method(arg)`, for the lang trait `lang` or the function at
    // `path` in `core`.
    let call = |ctx: &mut Ctx<'_>, callee: Res, arg: ExprId, at: TextRange| {
        let callee = ctx.alloc_expr(Expr::Path(callee), at);
        ctx.source_map.segments.insert(callee, at);
        ctx.alloc_expr(
            Expr::Call {
                callee,
                args: Box::new([arg]),
            },
            at,
        )
    };
    let lang_method = |lang: &str, method: &str| match items.lang_traits.get(&Symbol::intern(lang))
    {
        Some(&t) => Res::Assoc {
            ty: None,
            trait_: Some(t),
            name: Symbol::intern(method),
        },
        None => Res::Err,
    };
    let assert_clone = {
        let path = Path {
            anchor: PathAnchor::Global,
            qself: None,
            segments: ["core", "clone", "assert_param_is_clone"]
                .map(Symbol::intern)
                .into(),
            ranges: Box::default(),
            anchor_ranges: Box::default(),
            args: Box::default(),
        };
        match items.resolve(data.loc.module, &path, Ns::Values) {
            Ok(Resolved::Def(Def::Function(f))) => Res::Function(f),
            _ => Res::Err,
        }
    };
    let copies = derive == Derive::Copy;
    let mut arms = Vec::new();
    for (v, variant) in variants.iter().enumerate() {
        let v = v as u32;
        let (pat, mine) = variant_pat(&mut ctx, strukt, v, "__self_");
        let body = match derive {
            Derive::Clone => {
                let values: Vec<ExprId> = variant
                    .fields
                    .iter()
                    .zip(&mine)
                    .map(|(f, &b)| {
                        let arg = local(&mut ctx, b, f.def_range);
                        call(&mut ctx, lang_method("clone", "clone"), arg, f.def_range)
                    })
                    .collect();
                build_variant(&mut ctx, strukt, v, values, variant.def_range)
            }
            Derive::PartialEq => {
                let (other_pat, theirs) = variant_pat(&mut ctx, strukt, v, "__other_");
                let mut all: Option<ExprId> = None;
                for ((f, &a), &b) in variant.fields.iter().zip(&mine).zip(&theirs) {
                    let at = f.def_range;
                    let deref = |ctx: &mut Ctx<'_>, binding| {
                        let operand = local(ctx, binding, at);
                        ctx.alloc_expr(
                            Expr::Unary {
                                op: ast::UnaryOp::Deref,
                                operand,
                            },
                            at,
                        )
                    };
                    let (lhs, rhs) = (deref(&mut ctx, a), deref(&mut ctx, b));
                    let eq = ctx.alloc_expr(
                        Expr::Binary {
                            op: BinaryOp::Cmp(CmpOp::Eq),
                            lhs,
                            rhs,
                        },
                        at,
                    );
                    ctx.source_map.ops.insert(eq, at);
                    all = Some(match all {
                        None => eq,
                        Some(prev) => {
                            let and = ctx.alloc_expr(
                                Expr::Binary {
                                    op: BinaryOp::Logic(LogicOp::And),
                                    lhs: prev,
                                    rhs: eq,
                                },
                                at,
                            );
                            ctx.source_map.ops.insert(and, at);
                            and
                        }
                    });
                }
                let at = variant.def_range;
                let equal =
                    all.unwrap_or_else(|| ctx.alloc_expr(Expr::Literal(Literal::Bool(true)), at));
                // Another variant is unequal.
                let wild = ctx.alloc_pat(Pat::Wild, at);
                let unequal = ctx.alloc_expr(Expr::Literal(Literal::Bool(false)), at);
                let other = local(&mut ctx, receivers[1], at);
                let arms = Box::new([
                    MatchArm {
                        pat: other_pat,
                        guard: None,
                        body: equal,
                    },
                    MatchArm {
                        pat: wild,
                        guard: None,
                        body: unequal,
                    },
                ]);
                ctx.alloc_expr(
                    Expr::Match {
                        scrutinee: other,
                        arms,
                    },
                    at,
                )
            }
            Derive::Eq | Derive::Copy => {
                let check = if copies {
                    assert_clone
                } else {
                    lang_method("total_eq", "assert_receiver_is_total_eq")
                };
                let stmts: Box<[Stmt]> = variant
                    .fields
                    .iter()
                    .zip(&mine)
                    .map(|(f, &b)| {
                        let arg = local(&mut ctx, b, f.def_range);
                        Stmt::Expr {
                            expr: call(&mut ctx, check, arg, f.def_range),
                            has_semicolon: true,
                        }
                    })
                    .collect();
                ctx.alloc_expr(
                    Expr::Block {
                        stmts,
                        tail: None,
                        is_unsafe: false,
                    },
                    variant.def_range,
                )
            }
        };
        arms.push(MatchArm {
            pat,
            guard: None,
            body,
        });
    }
    let scrutinee = local(&mut ctx, receivers[0], whole);
    let matched = ctx.alloc_expr(
        Expr::Match {
            scrutinee,
            arms: arms.into(),
        },
        whole,
    );
    let root = if copies {
        // `{ match self { .. }; *self }`
        let this = local(&mut ctx, receivers[0], whole);
        let copy = ctx.alloc_expr(
            Expr::Unary {
                op: ast::UnaryOp::Deref,
                operand: this,
            },
            whole,
        );
        ctx.alloc_expr(
            Expr::Block {
                stmts: Box::new([Stmt::Expr {
                    expr: matched,
                    has_semicolon: true,
                }]),
                tail: Some(copy),
                is_unsafe: false,
            },
            whole,
        )
    } else {
        ctx.alloc_expr(
            Expr::Block {
                stmts: Box::default(),
                tail: Some(matched),
                is_unsafe: false,
            },
            whole,
        )
    };
    ctx.finish(params, root)
}

/// A pattern matching variant `v` of enum `strukt`, binding each field
/// to a name made from `prefix`, and the bindings, in field order.
fn variant_pat(
    ctx: &mut Ctx<'_>,
    strukt: StructId,
    v: u32,
    prefix: &str,
) -> (PatId, Vec<BindingId>) {
    let variant = &ctx.items.structs[strukt]
        .variants
        .as_deref()
        .unwrap_or_default()[v as usize];
    let mut bindings = Vec::new();
    let mut pats = Vec::new();
    for (i, f) in variant.fields.iter().enumerate() {
        let binding = ctx.bindings.alloc(Binding {
            name: Symbol::intern(&format!("{prefix}{i}")),
            mutable: false,
            by_ref: None,
        });
        bindings.push(binding);
        pats.push((
            f.name,
            ctx.alloc_pat(Pat::Bind { binding, sub: None }, f.def_range),
        ));
    }
    let at = variant.def_range;
    let pat = match variant.kind {
        StructKind::Tuple => Pat::TupleStruct {
            strukt,
            variant: v,
            pats: pats.into_iter().map(|(_, p)| p).collect(),
            rest: None,
        },
        StructKind::Record => Pat::Record {
            strukt,
            variant: v,
            fields: pats.into(),
            rest: false,
        },
        StructKind::Unit => Pat::Unit(strukt, v),
    };
    (ctx.alloc_pat(pat, at), bindings)
}

/// Variant `v` of enum `strukt` built from `values`, one per field.
fn build_variant(
    ctx: &mut Ctx<'_>,
    strukt: StructId,
    v: u32,
    values: Vec<ExprId>,
    at: TextRange,
) -> ExprId {
    let variant = &ctx.items.structs[strukt]
        .variants
        .as_deref()
        .unwrap_or_default()[v as usize];
    match variant.kind {
        StructKind::Unit => ctx.alloc_expr(Expr::Path(Res::Variant(strukt, v)), at),
        StructKind::Tuple => {
            let callee = ctx.alloc_expr(Expr::Path(Res::Variant(strukt, v)), at);
            ctx.alloc_expr(
                Expr::Call {
                    callee,
                    args: values.into(),
                },
                at,
            )
        }
        StructKind::Record => {
            let names: Vec<Symbol> = variant.fields.iter().map(|f| f.name).collect();
            let ranges: Box<[TextRange]> = variant.fields.iter().map(|f| f.name_range).collect();
            let id = ctx.alloc_expr(
                Expr::Record {
                    strukt,
                    variant: v,
                    fields: names.into_iter().zip(values).collect(),
                    spread: None,
                },
                at,
            );
            ctx.source_map.expr_fields.insert(id, ranges);
            ctx.source_map.paths.insert(id, at);
            id
        }
    }
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
    /// The bindings of the pattern being lowered so far.
    pat_binds: Vec<(Symbol, BindingId)>,
    /// For each or-pattern being lowered, the bindings of its first
    /// alternative, which the others bind again.
    or_reuse: Vec<Vec<(Symbol, BindingId)>>,
    pat_args: ArenaMap<Pat, PathGenerics>,
    /// [`Body::alt_by_ref`], and the entry for the binding pattern being
    /// lowered.
    alt_by_ref: ArenaMap<Pat, Option<bool>>,
    alt_mode: Option<Option<bool>>,
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
            pat_binds: Vec::new(),
            or_reuse: Vec::new(),
            pat_args: ArenaMap::new(),
            alt_by_ref: ArenaMap::new(),
            alt_mode: None,
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
                pat_generic_args: self.pat_args,
                alt_by_ref: self.alt_by_ref,
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
            ast::Expr::LetExpr(_) => {
                self.error(range, "expected expression, found `let` statement");
                Expr::Missing
            }
            ast::Expr::MacroExpr(_) => self.unsupported_expr(range, "macros"),
            ast::Expr::MatchExpr(m) => self.lower_match(m),
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
        let target = path
            .as_ref()
            .and_then(|path| self.resolve_struct_path(path, path_range, true));
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
        let Some((strukt, variant)) = target else {
            return self.alloc_expr(Expr::Missing, range);
        };
        let id = self.alloc_expr(
            Expr::Record {
                strukt,
                variant,
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
            let is_enum = self.items.structs[strukt].is_enum();
            if let Some(generics) = self.variant_generics(&path, path_range, is_enum) {
                self.generic_args.insert(id, generics);
            }
        }
        id
    }

    /// The generic arguments of a path to a struct, or to a variant if
    /// `is_enum`, which may follow the enum (`E::<T>::A`) or the variant
    /// (`E::A::<T>`). `Self` has none of its own.
    fn variant_generics(
        &mut self,
        path: &Path,
        range: TextRange,
        is_enum: bool,
    ) -> Option<PathGenerics> {
        let last = path.segments.len() - 1;
        let is_self = |i: usize| {
            path.anchor == PathAnchor::Plain && path.segments.get(i) == Some(&kw::SelfUpper)
        };
        let (item, ty) = match (is_enum, last) {
            (true, 0) => (Some(0), None),
            (true, _) => (Some(last), Some(last - 1)),
            (false, _) => (Some(last), None),
        };
        let self_type = match (is_enum, last) {
            (true, 0) => false,
            (true, _) => is_self(last - 1),
            (false, _) => is_self(last),
        };
        let item = item.filter(|&i| !is_self(i));
        let ty = ty.filter(|&i| !is_self(i));
        let mut generics = self.path_generics(path, range, item, ty, Res::Err);
        if self_type {
            generics.get_or_insert_default().self_type = true;
        }
        generics
    }

    /// The struct or variant a path in a pattern names, and the path.
    fn resolve_struct(
        &mut self,
        path: Option<ast::Path<'_>>,
        range: TextRange,
        record: bool,
    ) -> Option<(StructId, u32, Path)> {
        let range = path.map_or(range, |p| p.syntax().text_range());
        let path = path.and_then(|p| lower_generic_path(p, &mut self.diagnostics))?;
        path.reject_bindings(&mut self.diagnostics);
        let (strukt, variant) = self.resolve_struct_path(&path, range, record)?;
        Some((strukt, variant, path))
    }

    /// The struct, or the enum and variant, a path in a struct
    /// expression or pattern (with braces if `record`) names.
    fn resolve_struct_path(
        &mut self,
        path: &Path,
        range: TextRange,
        record: bool,
    ) -> Option<(StructId, u32)> {
        let found = self.resolve_struct_or_enum(path, range, record)?;
        match found {
            Ok(variant) => Some(variant),
            Err(e) => {
                let message = format!(
                    "expected struct, variant or union type, found enum `{}`",
                    if path.as_name() == Some(kw::SelfUpper) {
                        kw::SelfUpper
                    } else {
                        self.items.structs[e].name
                    }
                );
                self.error(range, message);
                None
            }
        }
    }

    /// What a struct path names: a struct or variant, or an enum (`Err`).
    fn resolve_struct_or_enum(
        &mut self,
        path: &Path,
        range: TextRange,
        record: bool,
    ) -> Option<Result<(StructId, u32), StructId>> {
        let found = |items: &ItemTree, strukt: StructId| {
            if items.structs[strukt].is_enum() {
                Err(strukt)
            } else {
                Ok((strukt, 0))
            }
        };
        // `Self::A`: a variant of the enum this block is for.
        if path.anchor == PathAnchor::Plain
            && path.segments.len() == 2
            && path.segments[0] == kw::SelfUpper
            && self.in_impl
        {
            let e = self.self_struct()?;
            let name = path.segments[1];
            return match self.items.structs[e].variant_named(name) {
                Some(v) => Some(Ok((e, v))),
                None => {
                    self.no_variant(e, name, path.range(1, range), record);
                    None
                }
            };
        }
        // `E::Z`, where `E` has no variant `Z`.
        if path.segments.len() >= 2 {
            let last = path.segments.len() - 1;
            let prefix = Path {
                segments: path.segments[..last].into(),
                ranges: path.ranges[..last.min(path.ranges.len())].into(),
                args: path.args[..last.min(path.args.len())].into(),
                ..path.clone()
            };
            if let Ok(Resolved::Def(Def::Struct(e))) =
                self.items.resolve(self.module, &prefix, Ns::Types)
                && self.items.structs[e].is_enum()
                && self.items.structs[e]
                    .variant_named(path.segments[last])
                    .is_none()
            {
                self.no_variant(e, path.segments[last], path.range(last, range), record);
                return None;
            }
        }
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
            return self.self_struct().map(|s| found(self.items, s));
        }
        match self.reported(
            self.items.resolve(self.module, path, Ns::Types),
            path,
            range,
        ) {
            Ok(Resolved::Def(Def::Struct(strukt))) => Some(found(self.items, strukt)),
            Ok(Resolved::Def(Def::Variant(e, v))) => Some(Ok((e, v))),
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

    /// Reports that `strukt`, an enum `Self` or a path names, has no
    /// variant `name`, as rustc does for a path with braces after it
    /// (`record`) or without.
    fn no_variant(&mut self, strukt: StructId, name: Symbol, at: TextRange, record: bool) {
        let data = &self.items.structs[strukt];
        let what = if data.is_enum() { "enum" } else { "struct" };
        let message = if record {
            format!("no variant named `{name}` found for {what} `{}`", data.name)
        } else {
            format!(
                "no variant, associated function, or constant named `{name}` found for {what} `{}` in the current scope",
                data.name
            )
        };
        self.error(at, message);
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
                    // The initializer is lowered before the pattern, whose
                    // bindings it cannot see: `let x = x + 1;`, and so is
                    // the `else` block.
                    let init = let_stmt.initializer().map(|e| self.lower_expr(e));
                    if let (Some(init), Some(_)) = (let_stmt.initializer(), let_stmt.let_else()) {
                        self.check_let_else_init(init);
                    }
                    let else_branch = let_stmt
                        .let_else()
                        .map(|e| self.lower_opt_block(e.block_expr(), e.syntax()));
                    if let Some(pat @ ast::Pat::OrPat(_)) = let_stmt.pat() {
                        self.error(
                            pat.syntax().text_range(),
                            "`let` bindings require top-level or-patterns in parentheses",
                        );
                    }
                    let ty = let_stmt.ty().map(|ty| {
                        let range = ty.syntax().text_range();
                        let ty = lower_type(Some(ty), &mut self.diagnostics);
                        let id = self.types.alloc(ty);
                        self.source_map.types.insert(id, range);
                        id
                    });
                    let pat = self.lower_top_pat(let_stmt.pat(), let_stmt.syntax(), PatSite::Let);
                    stmts.push(Stmt::Let {
                        pat,
                        ty,
                        init,
                        else_branch,
                    });
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
        let scope = self.scopes.len();
        let condition = self.lower_condition(if_expr.condition(), syntax);
        let then_branch = self.lower_opt_block(if_expr.then_branch(), syntax);
        self.scopes.truncate(scope);
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
        let scope = self.scopes.len();
        let condition = self.lower_condition(while_expr.condition(), syntax);
        let then_branch = self.lower_opt_block(while_expr.loop_body(), syntax);
        self.scopes.truncate(scope);
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

    /// What rustc forbids before the `else` of a `let`-`else`: an
    /// initializer ending in `}`, which reads as an `if`'s, and one that is
    /// `a && b` or `a || b`, which reads as a `let` chain.
    fn check_let_else_init(&mut self, init: ast::Expr<'_>) {
        let last = init
            .syntax()
            .tokens()
            .filter(|t| !t.kind().is_trivia())
            .last();
        if let Some(last) = last
            && last.kind() == flux_syntax::T!['}']
        {
            self.error(
                last.text_range(),
                "right curly brace `}` before `else` in a `let...else` statement not allowed",
            );
        } else if let ast::Expr::BinExpr(bin) = init
            && let Some(BinaryOp::Logic(op)) = bin.op_kind()
        {
            let op = if op == LogicOp::And { "&&" } else { "||" };
            self.error(
                init.syntax().text_range(),
                format!("a `{op}` expression cannot be directly assigned in `let...else`"),
            );
        }
    }

    /// The condition of an `if` or `while`: an expression, a `let`, or a
    /// chain of them joined by `&&`. The bindings of its `let`s stay in
    /// scope for the caller to end.
    fn lower_condition(&mut self, cond: Option<ast::Expr<'_>>, parent: SyntaxNode<'_>) -> ExprId {
        match cond {
            Some(ast::Expr::LetExpr(let_expr)) => {
                let range = let_expr.syntax().text_range();
                self.check_attrs(let_expr.syntax());
                let expr = self.lower_opt(let_expr.expr(), let_expr.syntax());
                let pat = self.lower_top_pat(let_expr.pat(), let_expr.syntax(), PatSite::Let);
                self.alloc_expr(Expr::Let { pat, expr }, range)
            }
            Some(ast::Expr::BinExpr(bin))
                if bin.op_kind() == Some(BinaryOp::Logic(LogicOp::And))
                    && Self::has_let(bin.syntax()) =>
            {
                let range = bin.syntax().text_range();
                self.check_attrs(bin.syntax());
                let lhs = self.lower_condition(bin.lhs(), bin.syntax());
                let rhs = self.lower_condition(bin.rhs(), bin.syntax());
                let id = self.alloc_expr(
                    Expr::Binary {
                        op: BinaryOp::Logic(LogicOp::And),
                        lhs,
                        rhs,
                    },
                    range,
                );
                if let Some(token) = bin.op() {
                    self.source_map.ops.insert(id, token.text_range());
                }
                id
            }
            Some(ast::Expr::BinExpr(bin))
                if bin.op_kind() == Some(BinaryOp::Logic(LogicOp::Or))
                    && Self::has_let(bin.syntax()) =>
            {
                let range = bin
                    .op()
                    .map_or(bin.syntax().text_range(), |t| t.text_range());
                self.error(
                    range,
                    "`||` operators are not supported in let chain conditions",
                );
                self.alloc_expr(Expr::Missing, bin.syntax().text_range())
            }
            Some(expr) => self.lower_expr(expr),
            None => self.alloc_expr(Expr::Missing, parent.text_range()),
        }
    }

    /// Whether a chain of `&&` or `||` has a `let` in it.
    fn has_let(node: SyntaxNode<'_>) -> bool {
        match ast::Expr::cast(node) {
            Some(ast::Expr::LetExpr(_)) => true,
            Some(ast::Expr::BinExpr(bin)) if matches!(bin.op_kind(), Some(BinaryOp::Logic(_))) => {
                bin.lhs().is_some_and(|e| Self::has_let(e.syntax()))
                    || bin.rhs().is_some_and(|e| Self::has_let(e.syntax()))
            }
            _ => false,
        }
    }

    /// `match scrutinee { pat if guard => body, .. }`
    fn lower_match(&mut self, m: ast::MatchExpr<'_>) -> Expr {
        let scrutinee = self.lower_opt(m.expr(), m.syntax());
        let mut arms = Vec::new();
        if let Some(list) = m.match_arm_list() {
            check_attrs(list.attrs(), &mut self.diagnostics);
            for arm in list.arms() {
                check_attrs(arm.attrs(), &mut self.diagnostics);
                let scope = self.scopes.len();
                let pat = self.lower_top_pat(arm.pat(), arm.syntax(), PatSite::Match);
                let guard = arm
                    .match_guard()
                    .map(|g| self.lower_condition(g.condition(), g.syntax()));
                let body = self.lower_opt(arm.expr(), arm.syntax());
                self.scopes.truncate(scope);
                arms.push(MatchArm { pat, guard, body });
            }
        }
        Expr::Match {
            scrutinee,
            arms: arms.into(),
        }
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
        self.lower_value_path(path.path(), path.syntax().text_range(), call)
    }

    /// A path in the value namespace, at `range`, as a `Path` expression.
    fn lower_value_path(
        &mut self,
        path: Option<ast::Path<'_>>,
        range: TextRange,
        call: Option<TextRange>,
    ) -> ExprId {
        let segment = path
            .filter(|p| p.qualifier().is_some())
            .and_then(|p| p.segment()?.name_ref())
            .map(|n| n.syntax().text_range());
        let (res, generics) = self.resolve_path(path, range, call);
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
        path: Option<ast::Path<'_>>,
        range: TextRange,
        call: Option<TextRange>,
    ) -> (Res, Option<PathGenerics>) {
        let Some(path) = path.and_then(|p| lower_generic_path(p, &mut self.diagnostics)) else {
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
            Res::Variant(..) if last > 0 => (Some(last), Some(last - 1)),
            Res::Variant(..) => (Some(last), None),
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
            let what = match res {
                Res::Const(c) if self.items.consts[c].owner.is_some() => "associated constant",
                _ => "constant",
            };
            self.error(
                path.range(last, range),
                format!(
                    "{what} takes 0 generic arguments but {got} generic argument{s} {verb} supplied"
                ),
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
                if self.items.structs[strukt].is_enum() {
                    self.error(
                        range,
                        "the `Self` constructor can only be used with tuple or unit structs",
                    );
                    return Res::Err;
                }
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
                            self.error(path.range(0, range), "cannot find `Self` in this scope");
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
            Ok(Resolved::Def(Def::Variant(e, v))) => Res::Variant(e, v),
            Ok(Resolved::Def(Def::Struct(strukt))) if self.items.structs[strukt].is_enum() => {
                self.error(
                    range,
                    format!(
                        "expected value, found enum `{}`",
                        path.segments[path.segments.len() - 1]
                    ),
                );
                Res::Err
            }
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
                // A struct with named fields is only in the type namespace,
                // so is a variant with them.
                let in_types = self.items.resolve(self.module, path, Ns::Types);
                if let Ok(Resolved::Def(Def::Variant(..))) = in_types {
                    self.error(
                        range,
                        format!("expected value, found struct variant `{path}`"),
                    );
                    return Res::Err;
                }
                let is_struct = matches!(in_types, Ok(Resolved::Def(Def::Struct(_))));
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
                if data.value(self.items, name).is_none() {
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

    /// The struct `Self` names here, as a value.
    fn self_type(&mut self, range: TextRange) -> Option<StructId> {
        if !self.in_impl {
            self.error(range, "cannot find value `Self` in this scope");
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

    /// A whole pattern: of a parameter, a `let`, a `match` arm or a
    /// `let` condition. Its bindings come into scope as they are made.
    fn lower_top_pat(
        &mut self,
        pat: Option<ast::Pat<'_>>,
        parent: SyntaxNode<'_>,
        site: PatSite,
    ) -> PatId {
        let saved = std::mem::take(&mut self.pat_binds);
        let id = self.lower_pat(pat, parent, site);
        self.pat_binds = saved;
        id
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
                let sub = ident
                    .pat()
                    .map(|sub| self.lower_pat(Some(sub), pat.syntax(), site));
                match ident.name() {
                    Some(name) => self.lower_ident_pat(name, ident, sub, site),
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
                let target = self.resolve_struct(tuple.path(), range, false);
                if let Some((strukt, _, _)) = target {
                    self.check_constructor_privacy(strukt, tuple.path(), range);
                }
                let (pats, rest) = self.lower_tuple_fields(tuple.fields(), site);
                let Some((strukt, variant, path)) = target else {
                    return self.alloc_pat(Pat::Missing, range);
                };
                let v = self.items.structs[strukt].variant(variant);
                if v.kind != StructKind::Tuple {
                    let message = match v.kind {
                        StructKind::Unit if v.enum_name.is_some() => format!(
                            "expected tuple struct or tuple variant, found unit variant `{path}`"
                        ),
                        StructKind::Unit => format!(
                            "expected tuple struct or tuple variant, found unit struct `{path}`"
                        ),
                        _ if v.enum_name.is_some() => format!(
                            "expected tuple struct or tuple variant, found struct variant `{path}`"
                        ),
                        _ => format!(
                            "cannot find tuple struct or tuple variant `{}` in this scope",
                            v.name
                        ),
                    };
                    self.error(range, message);
                    Pat::Missing
                } else {
                    let id = self.alloc_pat(
                        Pat::TupleStruct {
                            strukt,
                            variant,
                            pats,
                            rest,
                        },
                        range,
                    );
                    self.pat_generics(id, &path, tuple.path(), range);
                    return id;
                }
            }
            ast::Pat::RecordPat(record) => return self.lower_record_pat(record, site),
            ast::Pat::PathPat(path_pat) => {
                if self.names_value(path_pat.path()) {
                    return self.lower_const_pat(path_pat.path(), range);
                }
                let target = self.resolve_struct(path_pat.path(), range, false);
                match target {
                    Some((strukt, variant, path))
                        if self.items.structs[strukt].variant(variant).kind == StructKind::Unit =>
                    {
                        let id = self.alloc_pat(Pat::Unit(strukt, variant), range);
                        self.pat_generics(id, &path, path_pat.path(), range);
                        return id;
                    }
                    Some((strukt, variant, path)) => {
                        let v = self.items.structs[strukt].variant(variant);
                        let what = match (v.kind, v.enum_name.is_some()) {
                            (StructKind::Tuple, false) => "tuple struct",
                            (StructKind::Tuple, true) => "tuple variant",
                            (_, false) => "struct",
                            (_, true) => "struct variant",
                        };
                        let message = format!(
                            "expected unit struct, unit variant or constant, found {what} `{path}`"
                        );
                        self.error(range, message);
                        Pat::Missing
                    }
                    None => Pat::Missing,
                }
            }
            ast::Pat::LiteralPat(literal) => match self.lower_literal_pat(literal) {
                Some(expr) => Pat::Expr(expr),
                None => Pat::Missing,
            },
            ast::Pat::RangePat(range_pat) => self.lower_range_pat(range_pat),
            ast::Pat::OrPat(or) => self.lower_or_pat(or, site),
            ast::Pat::RefPat(r) => {
                let inner = self.lower_pat(r.pat(), pat.syntax(), site);
                Pat::Ref {
                    pat: inner,
                    mutable: r.mut_token().is_some(),
                }
            }
            ast::Pat::RestPat(_) => {
                self.error(range, "`..` patterns are not allowed here");
                Pat::Missing
            }
            _ => {
                let what = match pat {
                    ast::Pat::SlicePat(_) => "slice patterns",
                    _ => "macros",
                };
                self.unsupported(range, what);
                Pat::Missing
            }
        };
        let id = self.alloc_pat(hir, range);
        if let Some(by_ref) = self.alt_mode.take() {
            self.alt_by_ref.insert(id, by_ref);
        }
        id
    }

    /// Records the generic arguments of a struct or variant pattern's
    /// path.
    fn pat_generics(
        &mut self,
        id: PatId,
        path: &Path,
        node: Option<ast::Path<'_>>,
        range: TextRange,
    ) {
        if path.args.iter().all(Option::is_none) {
            return;
        }
        let range = node.map_or(range, |p| p.syntax().text_range());
        let strukt = match self.pats[id] {
            Pat::TupleStruct { strukt, .. } | Pat::Record { strukt, .. } | Pat::Unit(strukt, _) => {
                strukt
            }
            _ => return,
        };
        let is_enum = self.items.structs[strukt].is_enum();
        if let Some(generics) = self.variant_generics(path, range, is_enum) {
            self.pat_args.insert(id, generics);
        }
    }

    /// Whether `path`, in a pattern, names a constant, or something else
    /// in the value namespace that is not a struct or variant: what
    /// [`Self::lower_const_pat`] lowers.
    fn names_value(&self, path: Option<ast::Path<'_>>) -> bool {
        let Some(path) = path.and_then(|p| lower_generic_path(p, &mut Vec::new())) else {
            return false;
        };
        if path.qself.is_some() {
            return true;
        }
        let first = path.segments[0];
        if path.anchor == PathAnchor::Plain && path.segments.len() > 1 {
            // `T::N`, and `Self::N` in a trait.
            if self.generics.contains(&first) || (first == kw::SelfUpper && self.self_is_any) {
                return true;
            }
            if first == kw::SelfUpper {
                let variant = self
                    .self_struct()
                    .and_then(|e| self.items.structs[e].variant_named(path.segments[1]));
                return variant.is_none()
                    && self.self_target.is_some_and(|target| {
                        matches!(
                            self.items
                                .resolve_assoc(self.module, target, &path, 1, Ns::Values),
                            Ok(Resolved::AssocConst(_) | Resolved::Assoc(_))
                        )
                    });
            }
        }
        matches!(
            self.items.resolve(self.module, &path, Ns::Values),
            Ok(Resolved::Def(Def::Const(_) | Def::Function(_))
                | Resolved::AssocConst(_)
                | Resolved::Assoc(_)
                | Resolved::TraitItem(..)
                | Resolved::TypeRelative(_))
        )
    }

    /// A path pattern naming a constant, `N`, `i32::MAX` or `T::N`, which
    /// matches only the constant's value.
    fn lower_const_pat(&mut self, path: Option<ast::Path<'_>>, range: TextRange) -> PatId {
        let expr = self.lower_value_path(path, range, None);
        let hir = match self.exprs[expr] {
            Expr::Path(Res::Function(f)) => {
                let what = if self.items.functions[f].owner.is_some() {
                    "associated function"
                } else {
                    "function"
                };
                let path = path.map_or(String::new(), |p| p.syntax().text().to_string());
                self.error(
                    range,
                    format!(
                        "expected unit struct, unit variant or constant, found {what} `{path}`"
                    ),
                );
                Pat::Missing
            }
            Expr::Path(Res::Err) => Pat::Missing,
            _ => Pat::Expr(expr),
        };
        self.alloc_pat(hir, range)
    }

    /// `1`, `-1` or `true`, as an expression; other literals are
    /// reported.
    fn lower_literal_pat(&mut self, pat: ast::LiteralPat<'_>) -> Option<ExprId> {
        let range = pat.syntax().text_range();
        let literal = pat.literal()?;
        let lit_range = literal.syntax().text_range();
        let hir = self.lower_literal(literal);
        if hir == Expr::Missing {
            return None;
        }
        let expr = self.alloc_expr(hir, lit_range);
        if pat.minus_token().is_none() {
            return Some(expr);
        }
        Some(self.alloc_expr(
            Expr::Unary {
                op: ast::UnaryOp::Neg,
                operand: expr,
            },
            range,
        ))
    }

    /// `a..=b`, `a..b`, `a..` or `..=b`, whose ends are literals or
    /// paths to constants.
    fn lower_range_pat(&mut self, pat: ast::RangePat<'_>) -> Pat {
        let range = pat.syntax().text_range();
        let Some(op) = pat.op() else {
            return Pat::Missing;
        };
        if op.kind() == flux_syntax::T![...] {
            self.error(range, "`...` range patterns are deprecated");
            return Pat::Missing;
        }
        let inclusive = pat.op_kind() == Some(ast::RangeOp::Inclusive);
        let end = |this: &mut Self, end: Option<ast::Pat<'_>>| -> Result<Option<ExprId>, ()> {
            match end {
                None => Ok(None),
                Some(ast::Pat::LiteralPat(literal)) => {
                    this.lower_literal_pat(literal).map(Some).ok_or(())
                }
                Some(ast::Pat::PathPat(path)) => {
                    let at = path.syntax().text_range();
                    let local = path
                        .path()
                        .and_then(|p| lower_generic_path(p, &mut Vec::new()))
                        .and_then(|p| p.as_name())
                        .is_some_and(|name| this.scopes.iter().any(|&(n, _)| n == name));
                    if local {
                        this.error(at, "runtime values cannot be referenced in patterns");
                        return Err(());
                    }
                    let expr = this.lower_value_path(path.path(), at, None);
                    match this.exprs[expr] {
                        Expr::Path(Res::Err) => Err(()),
                        // Not a number: rustc says so of the range.
                        Expr::Path(Res::Function(_)) => {
                            this.error(
                                at,
                                "only `char` and numeric types are allowed in range patterns",
                            );
                            Err(())
                        }
                        _ => Ok(Some(expr)),
                    }
                }
                Some(other) => {
                    this.unsupported(other.syntax().text_range(), "this range pattern end");
                    Err(())
                }
            }
        };
        let (Ok(start), Ok(end_expr)) = (end(self, pat.start()), end(self, pat.end())) else {
            return Pat::Missing;
        };
        if inclusive && end_expr.is_none() {
            self.error(range, "inclusive range with no end");
            return Pat::Missing;
        }
        Pat::Range {
            start,
            end: end_expr,
            inclusive,
        }
    }

    /// `a | b`: the first alternative's bindings, which the others must
    /// bind too, and do with the same bindings.
    fn lower_or_pat(&mut self, or: ast::OrPat<'_>, site: PatSite) -> Pat {
        let base = self.pat_binds.len();
        let mut first: Option<(Vec<(Symbol, BindingId)>, TextRange)> = None;
        let mut alternatives = Vec::new();
        for alt in or.pats() {
            let alt_range = alt.syntax().text_range();
            self.or_reuse
                .push(first.as_ref().map(|(b, _)| b.clone()).unwrap_or_default());
            let id = self.lower_pat(Some(alt), or.syntax(), site);
            self.or_reuse.pop();
            let mine = self.pat_binds.split_off(base);
            match &first {
                None => first = Some((mine, alt_range)),
                Some((bound, first_range)) => {
                    for &(name, _) in bound {
                        if !mine.iter().any(|&(n, _)| n == name) {
                            self.error(
                                alt_range,
                                format!("variable `{name}` is not bound in all patterns"),
                            );
                        }
                    }
                    for &(name, _) in &mine {
                        if !bound.iter().any(|&(n, _)| n == name) {
                            self.error(
                                *first_range,
                                format!("variable `{name}` is not bound in all patterns"),
                            );
                        }
                    }
                }
            }
            alternatives.push(id);
        }
        if let Some((bound, _)) = first {
            self.pat_binds.extend(bound);
        }
        Pat::Or(alternatives.into())
    }

    /// `x`, `mut x`, `ref x`, `x @ pat`, or a unit struct's or unit
    /// variant's name, which is a pattern matching it rather than a new
    /// binding.
    fn lower_ident_pat(
        &mut self,
        name: ast::Name<'_>,
        ident: ast::IdentPat<'_>,
        sub: Option<PatId>,
        site: PatSite,
    ) -> Pat {
        let range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let plain = ident.mut_token().is_none() && ident.ref_token().is_none() && sub.is_none();
        let shadowed = match self.items.lookup(self.module, name, Ns::Values) {
            Some(Resolved::Def(Def::Struct(id))) => Some((id, 0)),
            Some(Resolved::Def(Def::Variant(id, v))) => Some((id, v)),
            // A constant's name is a pattern matching only its value.
            Some(Resolved::Def(Def::Const(c))) if plain => {
                let expr = self.alloc_expr(Expr::Path(Res::Const(c)), range);
                return Pat::Expr(expr);
            }
            Some(Resolved::Def(Def::Const(_))) => {
                let site = match site {
                    PatSite::Let => "let bindings",
                    PatSite::Param => "function parameters",
                    PatSite::Match => "match bindings",
                };
                self.error(range, format!("{site} cannot shadow constants"));
                None
            }
            _ => None,
        };
        if let Some((id, variant)) = shadowed {
            let v = self.items.structs[id].variant(variant);
            if v.kind == StructKind::Unit && plain {
                return Pat::Unit(id, variant);
            }
            let site = match site {
                PatSite::Let => "let bindings",
                PatSite::Param => "function parameters",
                PatSite::Match => "match bindings",
            };
            let what = match (v.kind, v.enum_name.is_some()) {
                (StructKind::Unit, false) => "unit structs",
                (StructKind::Unit, true) => "unit variants",
                (_, false) => "tuple structs",
                (_, true) => "tuple variants",
            };
            self.error(range, format!("{site} cannot shadow {what}"));
        }
        // A later alternative of an or-pattern binds the first's.
        let reused = self
            .or_reuse
            .iter()
            .rev()
            .find_map(|names| names.iter().find(|&&(n, _)| n == name))
            .map(|&(_, b)| b);
        // `ref mut x`'s `mut` follows `ref`; one before it, `mut ref x`,
        // makes the binding mutable, which is unstable.
        let ref_start = ident.ref_token().map(|t| t.text_range().start());
        let ref_mut = ref_start.is_some_and(|start| {
            ident
                .syntax()
                .children_with_tokens()
                .any(|t| t.kind() == flux_syntax::T![mut] && t.text_range().start() > start)
        });
        if ident
            .mut_token()
            .zip(ref_start)
            .is_some_and(|(m, start)| m.text_range().start() < start)
        {
            self.error(range, "mutable by-reference bindings are experimental");
        }
        let by_ref = ref_start.map(|_| ref_mut);
        let binding = match reused {
            Some(binding) => {
                let first = &self.bindings[binding];
                let mutable = ident.mut_token().is_some() && by_ref.is_none();
                if first.by_ref != by_ref {
                    self.alt_mode = Some(by_ref);
                }
                if first.by_ref != by_ref || first.mutable != mutable {
                    self.error(
                        range,
                        format!(
                            "variable `{name}` is bound inconsistently across alternatives separated by `|`"
                        ),
                    );
                }
                binding
            }
            None => {
                if self.pat_binds.iter().any(|&(n, _)| n == name) {
                    self.error(
                        range,
                        format!("identifier `{name}` is bound more than once in the same pattern"),
                    );
                }
                let binding = self.bindings.alloc(Binding {
                    name,
                    mutable: ident.mut_token().is_some(),
                    by_ref,
                });
                self.scopes.push((name, binding));
                binding
            }
        };
        if ident.ref_token().is_some() && ident.mut_token().is_some() {
            // `ref mut x`: the binding itself is not mutable.
            self.bindings[binding].mutable = false;
        }
        self.pat_binds.push((name, binding));
        Pat::Bind { binding, sub }
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

    /// `S { a, b: x, .. }` or `E::V { a, .. }`
    fn lower_record_pat(&mut self, record: ast::RecordPat<'_>, site: PatSite) -> PatId {
        let range = record.syntax().text_range();
        let target = self.resolve_struct(record.path(), range, true);
        let mut fields = Vec::new();
        let mut name_ranges = Vec::new();
        let mut rest = false;
        if let Some(list) = record.record_pat_field_list() {
            rest = list.rest_pat().is_some();
            // `..` must come last: rustc reports what follows it.
            if let Some(dots) = list.rest_pat() {
                let after = list
                    .syntax()
                    .tokens()
                    .filter(|t| !t.kind().is_trivia())
                    .find(|t| t.text_range().start() >= dots.syntax().text_range().end());
                if let Some(token) = after
                    && token.kind() != flux_syntax::T!['}']
                {
                    let message = format!("expected `}}`, found `{}`", token.text());
                    self.error(token.text_range(), message);
                }
            }
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
        let Some((strukt, variant, path)) = target else {
            return self.alloc_pat(Pat::Missing, range);
        };
        let id = self.alloc_pat(
            Pat::Record {
                strukt,
                variant,
                fields: fields.into(),
                rest,
            },
            range,
        );
        self.source_map.pat_fields.insert(id, name_ranges.into());
        self.pat_generics(id, &path, record.path(), range);
        id
    }
}
