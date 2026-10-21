//! The typed AST over real parses, mostly the hand-written accessors in
//! `src/ast/ext.rs`. The generated ones are checked against the grammar by
//! flux-sourcegen's tests.

use flux_syntax::ast::{self, AstNode, BinaryOp, CmpOp, ElseBranch, LiteralKind, RangeOp};
use flux_syntax::{Parse, SyntaxKind};

fn parse(text: &str) -> Parse {
    let parse = flux_syntax::parse_source_file(text);
    assert!(parse.errors().is_empty(), "{}", parse.debug_dump());
    parse
}

/// The first node of type `N` in `parse`.
fn find<'t, N: AstNode<'t>>(parse: &'t Parse) -> N {
    parse.root().descendants().find_map(N::cast).unwrap()
}

fn text<'t, N: AstNode<'t>>(node: Option<N>) -> &'t str {
    node.unwrap().syntax().text()
}

#[test]
fn function_signature() {
    let parse = parse("/// Doc.\npub const fn f<T>(x: T, y: u8) -> T where T: Copy { x }");
    let file = parse.source_file();
    let items: Vec<_> = file.items().collect();
    let [ast::Item::Fn(f)] = items[..] else {
        panic!("{items:?}")
    };
    assert_eq!(f.name().unwrap().text(), "f");
    assert!(f.visibility().is_some());
    assert!(f.const_token().is_some());
    assert!(f.unsafe_token().is_none());
    let params: Vec<_> = f.param_list().unwrap().params().collect();
    assert_eq!(params.len(), 2);
    assert_eq!(text(params[1].pat()), "y");
    assert_eq!(text(params[1].ty()), "u8");
    assert_eq!(text(f.ret_type().unwrap().ty()), "T");
    assert_eq!(f.where_clause().unwrap().predicates().count(), 1);
    assert_eq!(text(f.body().unwrap().tail_expr()), "x");
}

#[test]
fn binary_operands_follow_precedence() {
    let parse = parse("fn f() { a + b * c == d; }");
    let eq: ast::BinExpr = find(&parse);
    assert_eq!(eq.op_kind(), Some(BinaryOp::Cmp(CmpOp::Eq)));
    assert_eq!(text(eq.lhs()), "a + b * c");
    assert_eq!(text(eq.rhs()), "d");
    let Some(ast::Expr::BinExpr(add)) = eq.lhs() else {
        panic!()
    };
    assert_eq!(text(add.lhs()), "a");
    assert_eq!(text(add.rhs()), "b * c");
    assert_eq!(
        ast::BinExpr::cast(add.rhs().unwrap().syntax())
            .unwrap()
            .op_kind(),
        Some(BinaryOp::Arith(ast::ArithOp::Mul))
    );
}

#[test]
fn compound_assignment() {
    let parse = parse("fn f() { x <<= 1; }");
    let assign: ast::BinExpr = find(&parse);
    assert_eq!(
        assign.op_kind(),
        Some(BinaryOp::Assign(Some(ast::ArithOp::Shl)))
    );
    assert_eq!(text(assign.lhs()), "x");
    assert_eq!(text(assign.rhs()), "1");
}

#[test]
fn ranges_with_missing_ends() {
    let parse = parse("fn f() { (a..b, ..c, d.., ..=e, ..); }");
    let ranges: Vec<ast::RangeExpr> = parse
        .root()
        .descendants()
        .filter_map(ast::RangeExpr::cast)
        .collect();
    let ends: Vec<_> = ranges
        .iter()
        .map(|r| {
            (
                r.start().map(|e| e.syntax().text()),
                r.end().map(|e| e.syntax().text()),
                r.op_kind().unwrap(),
            )
        })
        .collect();
    use RangeOp::*;
    assert_eq!(
        ends,
        [
            (Some("a"), Some("b"), Exclusive),
            (None, Some("c"), Exclusive),
            (Some("d"), None, Exclusive),
            (None, Some("e"), Inclusive),
            (None, None, Exclusive),
        ]
    );
}

#[test]
fn range_patterns() {
    let parse = parse("fn f() { match x { 0..=9 => {} ..0 => {} _ => {} } }");
    let pat: ast::RangePat = find(&parse);
    assert_eq!(text(pat.start()), "0");
    assert_eq!(text(pat.end()), "9");
    assert_eq!(pat.op_kind(), Some(RangeOp::Inclusive));
}

#[test]
fn if_else_chains() {
    let parse = parse("fn f() { if a { 1 } else if { b } { 2 } else { 3 } }");
    let outer: ast::IfExpr = find(&parse);
    assert_eq!(text(outer.condition()), "a");
    assert_eq!(text(outer.then_branch()), "{ 1 }");
    let Some(ElseBranch::If(inner)) = outer.else_branch() else {
        panic!()
    };
    // A block is a valid condition; position tells it from the branch.
    assert_eq!(text(inner.condition()), "{ b }");
    assert_eq!(text(inner.then_branch()), "{ 2 }");
    let Some(ElseBranch::Block(last)) = inner.else_branch() else {
        panic!()
    };
    assert_eq!(last.syntax().text(), "{ 3 }");
}

#[test]
fn loops() {
    let parse = parse("fn f() { 'l: for i in 0..n { } while c { } loop { } }");
    let for_expr: ast::ForExpr = find(&parse);
    assert_eq!(text(for_expr.label().unwrap().lifetime()), "'l");
    assert_eq!(text(for_expr.pat()), "i");
    assert_eq!(text(for_expr.iterable()), "0..n");
    assert_eq!(text(for_expr.loop_body()), "{ }");
    let while_expr: ast::WhileExpr = find(&parse);
    assert_eq!(text(while_expr.condition()), "c");
    assert_eq!(text(while_expr.loop_body()), "{ }");
    let loop_expr: ast::LoopExpr = find(&parse);
    assert_eq!(text(loop_expr.loop_body()), "{ }");
}

#[test]
fn block_statements_and_tail() {
    let parse = parse("fn f() { let x: u8 = 1; ; g(); struct S; x }");
    let block = find::<ast::Fn>(&parse).body().unwrap();
    let kinds: Vec<_> = block.stmts().map(|s| s.syntax().kind()).collect();
    use SyntaxKind::*;
    assert_eq!(kinds, [LET_STMT, EXPR_STMT, STRUCT]);
    assert_eq!(text(block.tail_expr()), "x");
    let let_stmt: ast::LetStmt = find(&parse);
    assert_eq!(text(let_stmt.pat()), "x");
    assert_eq!(text(let_stmt.ty()), "u8");
    assert_eq!(text(let_stmt.initializer()), "1");
}

#[test]
fn indexing_and_calls() {
    let parse = parse("fn f() { v[i + 1].push::<u8>(x, y); }");
    let index: ast::IndexExpr = find(&parse);
    assert_eq!(text(index.base()), "v");
    assert_eq!(text(index.index()), "i + 1");
    let call: ast::MethodCallExpr = find(&parse);
    assert_eq!(text(call.receiver()), "v[i + 1]");
    assert_eq!(call.name_ref().unwrap().text(), "push");
    assert_eq!(call.generic_arg_list().unwrap().args().count(), 1);
    assert_eq!(call.arg_list().unwrap().args().count(), 2);
}

#[test]
fn impl_headers() {
    let parse = parse("impl<T> Clone for Vec<T> {} impl S {}");
    let impls: Vec<ast::Impl> = parse
        .root()
        .descendants()
        .filter_map(ast::Impl::cast)
        .collect();
    assert_eq!(text(impls[0].trait_ty()), "Clone");
    assert_eq!(text(impls[0].self_ty()), "Vec<T>");
    assert!(impls[1].trait_ty().is_none());
    assert_eq!(text(impls[1].self_ty()), "S");
}

#[test]
fn qualified_paths() {
    let parse = parse("type A = <Vec<T> as IntoIterator>::Item;");
    let path = find::<ast::TypeAlias>(&parse).ty().unwrap();
    let ast::Type::PathType(path) = path else {
        panic!()
    };
    let path = path.path().unwrap();
    assert_eq!(text(path.segment().unwrap().name_ref()), "Item");
    let qualifier = path.qualifier().unwrap().segment().unwrap();
    assert_eq!(text(qualifier.self_ty()), "Vec<T>");
    assert_eq!(text(qualifier.trait_ty()), "IntoIterator");
}

#[test]
fn literals() {
    let parse = parse(r#"const X: [u8; 4] = [1, 2.0 as u8, b'c', true as u8];"#);
    let kinds: Vec<_> = parse
        .root()
        .descendants()
        .filter_map(ast::Literal::cast)
        .map(|l| l.kind().unwrap())
        .collect();
    use LiteralKind::*;
    assert_eq!(kinds, [Int, Int, Float, Byte, Bool(true)]);
}

#[test]
fn macros_by_position() {
    let parse = parse("m!(); fn f(x: t!()) { let p!() = e!(); }");
    let file = parse.source_file();
    assert!(matches!(file.items().next(), Some(ast::Item::MacroCall(_))));
    assert_eq!(text(find::<ast::MacroType>(&parse).path()), "t");
    let let_stmt: ast::LetStmt = find(&parse);
    assert!(matches!(let_stmt.pat(), Some(ast::Pat::MacroPat(_))));
    assert!(matches!(
        let_stmt.initializer(),
        Some(ast::Expr::MacroExpr(_))
    ));
}

#[test]
fn records() {
    let parse = parse(
        "struct P { pub x: i32, y: i32 } fn f(p: P) { let P { x, y: _ } = P { x: 1, ..p }; }",
    );
    let fields: Vec<_> = find::<ast::Struct>(&parse)
        .record_field_list()
        .unwrap()
        .fields()
        .map(|f| f.name().unwrap().text())
        .collect();
    assert_eq!(fields, ["x", "y"]);
    let pat: ast::RecordPat = find(&parse);
    let names: Vec<_> = pat
        .record_pat_field_list()
        .unwrap()
        .fields()
        .map(|f| f.pat().unwrap().syntax().text())
        .collect();
    assert_eq!(names, ["x", "_"]);
    let expr: ast::RecordExpr = find(&parse);
    let list = expr.record_expr_field_list().unwrap();
    assert_eq!(list.fields().count(), 1);
    assert_eq!(text(list.spread()), "p");
}
