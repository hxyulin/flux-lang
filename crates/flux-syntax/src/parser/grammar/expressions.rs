//! Expressions and statements.
//!
//! Binary operators are parsed by precedence climbing ("Pratt parsing"):
//! [`expr_bp`] parses an operand, then keeps absorbing operators that bind
//! at least as tightly as `min_bp`, wrapping what it has so far with
//! [`CompletedMarker::precede`].
//!
//! Two context restrictions come from Rust's grammar:
//!
//! - In `if`, `while`, `for` and `match` heads a struct literal is not
//!   allowed, so `if x == S {}` compares with the path `S` and the `{` opens
//!   the body.
//! - At the start of a statement a block-like expression (`if`, `match`,
//!   `{}`, loops) ends the statement: `if c {} - 1` is two statements. A
//!   method call may still follow, as in `match x {}.unwrap();`.

use super::*;

#[derive(Clone, Copy)]
struct Restrictions {
    forbid_structs: bool,
    prefer_stmt: bool,
}

const DEFAULT: Restrictions = Restrictions {
    forbid_structs: false,
    prefer_stmt: false,
};

const NO_STRUCT: Restrictions = Restrictions {
    forbid_structs: true,
    prefer_stmt: false,
};

const STMT: Restrictions = Restrictions {
    forbid_structs: false,
    prefer_stmt: true,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockLike {
    Block,
    NotBlock,
}

/// Binding power of unary operators: tighter than every binary operator,
/// looser than postfix ones (`-a.b()` is `-(a.b())`).
const UNARY_BP: u8 = 13;

pub(super) const LITERAL_FIRST: TokenSet = TokenSet::new(&[
    INT_NUMBER,
    FLOAT_NUMBER,
    CHAR,
    BYTE,
    STRING,
    BYTE_STRING,
    C_STRING,
    T![true],
    T![false],
]);

pub(super) const EXPR_FIRST: TokenSet =
    LITERAL_FIRST
        .union(paths::PATH_FIRST)
        .union(TokenSet::new(&[
            T!['('],
            T!['['],
            T!['{'],
            T![|],
            T![&],
            T![*],
            T![!],
            T![-],
            T![.],
            T![_],
            LIFETIME_IDENT,
            T![if],
            T![while],
            T![loop],
            T![for],
            T![match],
            T![break],
            T![continue],
            T![return],
            T![let],
            T![unsafe],
            T![const],
            T![async],
            T![move],
            T![#],
        ]));

const EXPR_RECOVERY_SET: TokenSet = TokenSet::new(&[T![let], T![;], T![,], T![')'], T![']']]);

pub(super) fn expr(p: &mut Parser<'_>) {
    expr_bp(p, DEFAULT, 1);
}

fn expr_no_struct(p: &mut Parser<'_>) {
    expr_bp(p, NO_STRUCT, 1);
}

pub(super) fn literal(p: &mut Parser<'_>) {
    if !p.at_ts(LITERAL_FIRST) {
        p.error("expected a literal");
        return;
    }
    let m = p.start();
    p.bump_any();
    m.complete(p, LITERAL);
}

fn at_expr_start(p: &mut Parser<'_>, r: Restrictions) -> bool {
    p.at_ts(EXPR_FIRST) && !(r.forbid_structs && p.at(T!['{']))
}

// Statements.

/// `{ statements; tail }`.
pub(super) fn block_expr(p: &mut Parser<'_>) {
    if !p.at(T!['{']) {
        p.error("expected `{`");
        return;
    }
    let m = p.start();
    stmt_list(p);
    m.complete(p, BLOCK_EXPR);
}

fn stmt_list(p: &mut Parser<'_>) {
    p.bump(T!['{']);
    attributes::inner_attrs(p);
    while !p.at(T!['}']) && !p.at_end() {
        stmt(p);
    }
    p.expect(T!['}']);
}

/// One statement. An expression at the end of a block without a `;` is
/// the block's value and is not wrapped in `EXPR_STMT`, unless it has
/// attributes.
fn stmt(p: &mut Parser<'_>) {
    if p.eat(T![;]) {
        return;
    }
    let m = p.start();
    let has_attrs = p.at(T![#]);
    attributes::outer_attrs(p);
    if p.at(T![let]) {
        let_stmt(p, m);
        return;
    }
    let m = match items::opt_item(p, m) {
        Ok(()) => return,
        Err(m) => m,
    };
    if !p.at_ts(EXPR_FIRST) {
        if has_attrs {
            p.error("expected a statement after attributes");
            m.complete(p, ERROR);
        } else {
            m.abandon(p);
            p.err_and_bump("expected a statement");
        }
        return;
    }
    let Some((_, blocklike)) = expr_bp(p, STMT, 1) else {
        if has_attrs {
            m.complete(p, ERROR);
        } else {
            m.abandon(p);
        }
        return;
    };
    if p.at(T!['}']) && !has_attrs {
        m.abandon(p);
        return;
    }
    if !p.eat(T![;]) && blocklike == BlockLike::NotBlock && !p.at(T!['}']) {
        p.error("expected `;`");
    }
    m.complete(p, EXPR_STMT);
}

/// `let pattern: Type = init else { diverge };`.
fn let_stmt(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![let]);
    patterns::pattern(p);
    if p.eat(T![:]) {
        types::type_(p);
    }
    if p.eat(T![=]) {
        expr(p);
        if p.at(T![else]) {
            let e = p.start();
            p.bump(T![else]);
            block_expr(p);
            e.complete(p, LET_ELSE);
        }
    }
    p.expect(T![;]);
    m.complete(p, LET_STMT);
}

// Operators.

fn expr_bp(
    p: &mut Parser<'_>,
    r: Restrictions,
    min_bp: u8,
) -> Option<(CompletedMarker, BlockLike)> {
    if p.at(T![#]) {
        // Outer attributes belong to the whole expression that follows.
        let m = p.start();
        attributes::outer_attrs(p);
        return match expr_bp(p, r, min_bp) {
            Some((cm, blocklike)) => Some((cm.extend_to(p, m), blocklike)),
            None => {
                m.abandon(p);
                None
            }
        };
    }
    let (mut lhs, mut blocklike) = lhs(p, r)?;
    if r.prefer_stmt && blocklike == BlockLike::Block {
        if !(p.at(T![.]) && !p.at(T![..])) && !p.at(T![?]) {
            return Some((lhs, BlockLike::Block));
        }
        lhs = postfix(p, lhs);
        blocklike = BlockLike::NotBlock;
    }
    let operand_r = Restrictions {
        prefer_stmt: false,
        ..r
    };
    loop {
        let (bp, op) = current_op(p);
        if bp == 0 || bp < min_bp {
            break;
        }
        blocklike = BlockLike::NotBlock;
        let m = lhs.precede(p);
        p.bump(op);
        match op {
            T![as] => {
                types::type_no_bounds(p);
                lhs = m.complete(p, CAST_EXPR);
            }
            T![..] | T![..=] => {
                if at_expr_start(p, r) {
                    expr_bp(p, operand_r, bp + 1);
                } else if op == T![..=] {
                    p.error("expected the end of the range");
                }
                lhs = m.complete(p, RANGE_EXPR);
            }
            _ => {
                // Assignment is right-associative: `a = b = c` is
                // `a = (b = c)`. Everything else is left-associative.
                let rhs_bp = if is_assignment(op) { bp } else { bp + 1 };
                expr_bp(p, operand_r, rhs_bp);
                lhs = m.complete(p, BIN_EXPR);
            }
        }
    }
    Some((lhs, blocklike))
}

/// The binary operator at the current position and its binding power, or
/// a binding power of 0 when there is none. Higher binds tighter.
fn current_op(p: &mut Parser<'_>) -> (u8, SyntaxKind) {
    const NOT_AN_OP: (u8, SyntaxKind) = (0, T![@]);
    match p.current() {
        T![=] if p.at(T![==]) => (5, T![==]),
        T![=] if p.at(T![=>]) => NOT_AN_OP,
        T![=] => (1, T![=]),
        T![!] if p.at(T![!=]) => (5, T![!=]),
        T![<] if p.at(T![<<=]) => (1, T![<<=]),
        T![<] if p.at(T![<<]) => (9, T![<<]),
        T![<] if p.at(T![<=]) => (5, T![<=]),
        T![<] => (5, T![<]),
        T![>] if p.at(T![>>=]) => (1, T![>>=]),
        T![>] if p.at(T![>>]) => (9, T![>>]),
        T![>] if p.at(T![>=]) => (5, T![>=]),
        T![>] => (5, T![>]),
        T![|] if p.at(T![||]) => (3, T![||]),
        T![|] if p.at(T![|=]) => (1, T![|=]),
        T![|] => (6, T![|]),
        T![&] if p.at(T![&&]) => (4, T![&&]),
        T![&] if p.at(T![&=]) => (1, T![&=]),
        T![&] => (8, T![&]),
        T![^] if p.at(T![^=]) => (1, T![^=]),
        T![^] => (7, T![^]),
        T![+] if p.at(T![+=]) => (1, T![+=]),
        T![+] => (10, T![+]),
        T![-] if p.at(T![-=]) => (1, T![-=]),
        T![-] if p.at(T![->]) => NOT_AN_OP,
        T![-] => (10, T![-]),
        T![*] if p.at(T![*=]) => (1, T![*=]),
        T![*] => (11, T![*]),
        T![/] if p.at(T![/=]) => (1, T![/=]),
        T![/] => (11, T![/]),
        T![%] if p.at(T![%=]) => (1, T![%=]),
        T![%] => (11, T![%]),
        T![.] if p.at(T![..=]) => (2, T![..=]),
        T![.] if p.at(T![..]) => (2, T![..]),
        T![as] => (12, T![as]),
        _ => NOT_AN_OP,
    }
}

fn is_assignment(op: SyntaxKind) -> bool {
    matches!(
        op,
        T![=]
            | T![+=]
            | T![-=]
            | T![*=]
            | T![/=]
            | T![%=]
            | T![^=]
            | T![&=]
            | T![|=]
            | T![<<=]
            | T![>>=]
    )
}

/// A prefix-operator expression, a prefix range, or an atom with its
/// postfix operators.
fn lhs(p: &mut Parser<'_>, r: Restrictions) -> Option<(CompletedMarker, BlockLike)> {
    let operand_r = Restrictions {
        prefer_stmt: false,
        ..r
    };
    let m;
    let kind = match p.current() {
        T![&] => {
            m = p.start();
            p.bump(T![&]);
            p.eat(T![mut]);
            REF_EXPR
        }
        T![*] | T![!] | T![-] => {
            m = p.start();
            p.bump_any();
            PREFIX_EXPR
        }
        T![.] if p.at(T![..=]) || p.at(T![..]) => {
            // `..`, `..end` and `..=end`.
            m = p.start();
            if !p.eat(T![..=]) {
                p.bump(T![..]);
            }
            if at_expr_start(p, r) {
                expr_bp(p, operand_r, 3);
            }
            return Some((m.complete(p, RANGE_EXPR), BlockLike::NotBlock));
        }
        _ => {
            let (atom, blocklike) = atom_expr(p, r)?;
            if r.prefer_stmt && blocklike == BlockLike::Block {
                return Some((atom, blocklike));
            }
            return Some((postfix(p, atom), BlockLike::NotBlock));
        }
    };
    expr_bp(p, operand_r, UNARY_BP);
    Some((m.complete(p, kind), BlockLike::NotBlock))
}

/// Calls, indexing, `?`, fields, methods and `.await` after `lhs`.
fn postfix(p: &mut Parser<'_>, mut lhs: CompletedMarker) -> CompletedMarker {
    loop {
        lhs = match p.current() {
            T!['('] => {
                let m = lhs.precede(p);
                arg_list(p);
                m.complete(p, CALL_EXPR)
            }
            T!['['] => {
                let m = lhs.precede(p);
                p.bump(T!['[']);
                expr(p);
                p.expect(T![']']);
                m.complete(p, INDEX_EXPR)
            }
            T![?] => {
                let m = lhs.precede(p);
                p.bump(T![?]);
                m.complete(p, TRY_EXPR)
            }
            T![.] if !p.at(T![..]) => dot_expr(p, lhs),
            _ => return lhs,
        };
    }
}

fn dot_expr(p: &mut Parser<'_>, lhs: CompletedMarker) -> CompletedMarker {
    let m = lhs.precede(p);
    p.bump(T![.]);
    match p.current() {
        T![await] => {
            p.bump(T![await]);
            m.complete(p, AWAIT_EXPR)
        }
        IDENT if p.nth(1) == T!['('] || p.nth_at(1, T![::]) => {
            name_ref(p);
            paths::opt_turbofish(p);
            if p.at(T!['(']) {
                arg_list(p);
            } else {
                p.error("expected `(`");
            }
            m.complete(p, METHOD_CALL_EXPR)
        }
        IDENT | INT_NUMBER => {
            name_ref_or_index(p);
            m.complete(p, FIELD_EXPR)
        }
        _ => {
            p.error("expected a field or method name");
            m.complete(p, FIELD_EXPR)
        }
    }
}

fn arg_list(p: &mut Parser<'_>) {
    let m = p.start();
    delimited(p, T!['('], T![')'], EXPR_FIRST, "an argument", expr);
    m.complete(p, ARG_LIST);
}

// Atoms.

fn atom_expr(p: &mut Parser<'_>, r: Restrictions) -> Option<(CompletedMarker, BlockLike)> {
    use BlockLike::*;
    if p.at_ts(LITERAL_FIRST) {
        let m = p.start();
        p.bump_any();
        return Some((m.complete(p, LITERAL), NotBlock));
    }
    if paths::is_path_start(p) {
        return Some(path_expr(p, r));
    }
    let done = match p.current() {
        T!['('] => (tuple_or_paren_expr(p), NotBlock),
        T!['['] => (array_expr(p), NotBlock),
        T![|] => (closure_expr(p, r), NotBlock),
        T![move] if p.nth(1) == T![|] => (closure_expr(p, r), NotBlock),
        T![async] if p.nth(1) == T![|] || (p.nth(1) == T![move] && p.nth(2) == T![|]) => {
            (closure_expr(p, r), NotBlock)
        }
        T![if] => (if_expr(p), Block),
        LIFETIME_IDENT if p.nth(1) == T![:] => {
            let m = p.start();
            let label = p.start();
            lifetime(p);
            p.bump(T![:]);
            label.complete(p, LABEL);
            (loop_or_block(p, m), Block)
        }
        T![loop] | T![while] | T![for] | T!['{'] => {
            let m = p.start();
            (loop_or_block(p, m), Block)
        }
        T![unsafe] | T![const] | T![async] => {
            let m = p.start();
            p.bump_any();
            if p.at(T![move]) && p.nth(1) == T!['{'] {
                p.bump(T![move]);
            }
            if p.at(T!['{']) {
                stmt_list(p);
            } else {
                p.error("expected `{`");
            }
            (m.complete(p, BLOCK_EXPR), Block)
        }
        T![match] => (match_expr(p), Block),
        T![return] => {
            let m = p.start();
            p.bump(T![return]);
            if at_expr_start(p, r) {
                expr_bp(
                    p,
                    Restrictions {
                        prefer_stmt: false,
                        ..r
                    },
                    1,
                );
            }
            (m.complete(p, RETURN_EXPR), NotBlock)
        }
        T![break] => {
            let m = p.start();
            p.bump(T![break]);
            if p.at(LIFETIME_IDENT) {
                lifetime(p);
            }
            if at_expr_start(p, r) {
                expr_bp(
                    p,
                    Restrictions {
                        prefer_stmt: false,
                        ..r
                    },
                    1,
                );
            }
            (m.complete(p, BREAK_EXPR), NotBlock)
        }
        T![continue] => {
            let m = p.start();
            p.bump(T![continue]);
            if p.at(LIFETIME_IDENT) {
                lifetime(p);
            }
            (m.complete(p, CONTINUE_EXPR), NotBlock)
        }
        T![let] => {
            // `let` is an expression only in `if` and `while` conditions,
            // where it may be chained with `&&`. Its scrutinee binds more
            // tightly than `&&` and `||`; other positions are rejected
            // after parsing.
            let m = p.start();
            p.bump(T![let]);
            patterns::pattern(p);
            p.expect(T![=]);
            expr_bp(
                p,
                Restrictions {
                    prefer_stmt: false,
                    ..r
                },
                5,
            );
            (m.complete(p, LET_EXPR), NotBlock)
        }
        T![_] => {
            let m = p.start();
            p.bump(T![_]);
            (m.complete(p, UNDERSCORE_EXPR), NotBlock)
        }
        _ => {
            p.err_recover("expected an expression", EXPR_RECOVERY_SET);
            return None;
        }
    };
    Some(done)
}

/// A path, a macro call `path!(...)`, or a struct literal `Path { ... }`.
fn path_expr(p: &mut Parser<'_>, r: Restrictions) -> (CompletedMarker, BlockLike) {
    let m = p.start();
    paths::expr_path(p);
    match p.current() {
        T![!] if !p.at(T![!=]) => {
            p.bump(T![!]);
            if p.at(IDENT) {
                // `macro_rules! name { ... }`
                name(p);
            }
            let blocklike = if p.at(T!['{']) {
                BlockLike::Block
            } else {
                BlockLike::NotBlock
            };
            if matches!(p.current(), T!['('] | T!['['] | T!['{']) {
                attributes::token_tree(p);
            } else {
                p.error("expected `(`, `[` or `{`");
            }
            (m.complete(p, MACRO_EXPR), blocklike)
        }
        T!['{'] if !r.forbid_structs => {
            record_expr_field_list(p);
            (m.complete(p, RECORD_EXPR), BlockLike::NotBlock)
        }
        _ => (m.complete(p, PATH_EXPR), BlockLike::NotBlock),
    }
}

/// `{ a, b: 1, 0: x, ..base }`.
fn record_expr_field_list(p: &mut Parser<'_>) {
    let list = p.start();
    p.bump(T!['{']);
    while !p.at(T!['}']) && !p.at_end() {
        if p.at(T![..]) {
            p.bump(T![..]);
            if at_expr_start(p, DEFAULT) {
                expr(p);
            }
            continue;
        }
        if !matches!(p.current(), IDENT | INT_NUMBER | T![#]) {
            p.err_and_bump("expected a field");
            continue;
        }
        let m = p.start();
        attributes::outer_attrs(p);
        if p.nth(1) == T![:] && !p.nth_at(1, T![::]) {
            name_ref_or_index(p);
            p.bump(T![:]);
            expr(p);
        } else {
            name_ref_or_index(p);
        }
        m.complete(p, RECORD_EXPR_FIELD);
        if !p.at(T!['}']) && !p.expect(T![,]) {
            break;
        }
    }
    p.expect(T!['}']);
    list.complete(p, RECORD_EXPR_FIELD_LIST);
}

/// `()`, `(e)`, `(e,)` or `(a, b)`.
fn tuple_or_paren_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(T!['(']);
    let mut count = 0;
    let mut trailing_comma = false;
    while !p.at(T![')']) && !p.at_end() && p.at_ts(EXPR_FIRST) {
        expr(p);
        count += 1;
        trailing_comma = p.eat(T![,]);
        if !trailing_comma {
            break;
        }
    }
    p.expect(T![')']);
    let kind = if count == 1 && !trailing_comma {
        PAREN_EXPR
    } else {
        TUPLE_EXPR
    };
    m.complete(p, kind)
}

/// `[a, b, c]` or `[value; count]`.
fn array_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(T!['[']);
    if p.at_ts(EXPR_FIRST) {
        expr(p);
        if p.eat(T![;]) {
            expr(p);
        } else {
            while p.eat(T![,]) && !p.at(T![']']) {
                if !p.at_ts(EXPR_FIRST) {
                    p.error("expected an expression");
                    break;
                }
                expr(p);
            }
        }
    }
    p.expect(T![']']);
    m.complete(p, ARRAY_EXPR)
}

/// `move |a, b: B| body` or `|| -> T { body }`.
fn closure_expr(p: &mut Parser<'_>, r: Restrictions) -> CompletedMarker {
    let m = p.start();
    p.eat(T![async]);
    p.eat(T![move]);
    params::closure_param_list(p);
    if opt_ret_type(p, true) {
        // With an explicit return type the body must be a block.
        block_expr(p);
    } else {
        expr_bp(
            p,
            Restrictions {
                prefer_stmt: false,
                ..r
            },
            1,
        );
    }
    m.complete(p, CLOSURE_EXPR)
}

/// `if cond { } else if cond { } else { }`.
fn if_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(T![if]);
    expr_no_struct(p);
    block_expr(p);
    if p.eat(T![else]) {
        if p.at(T![if]) {
            if_expr(p);
        } else {
            block_expr(p);
        }
    }
    m.complete(p, IF_EXPR)
}

/// A loop or a plain block, into `m`, which may already hold a label.
fn loop_or_block(p: &mut Parser<'_>, m: Marker) -> CompletedMarker {
    match p.current() {
        T![loop] => {
            p.bump(T![loop]);
            block_expr(p);
            m.complete(p, LOOP_EXPR)
        }
        T![while] => {
            p.bump(T![while]);
            expr_no_struct(p);
            block_expr(p);
            m.complete(p, WHILE_EXPR)
        }
        T![for] => {
            p.bump(T![for]);
            patterns::pattern(p);
            p.expect(T![in]);
            expr_no_struct(p);
            block_expr(p);
            m.complete(p, FOR_EXPR)
        }
        T!['{'] => {
            stmt_list(p);
            m.complete(p, BLOCK_EXPR)
        }
        _ => {
            p.error("expected a loop or block after the label");
            m.complete(p, BLOCK_EXPR)
        }
    }
}

/// `match scrutinee { pattern if guard => body, ... }`.
fn match_expr(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    p.bump(T![match]);
    expr_no_struct(p);
    if p.at(T!['{']) {
        match_arm_list(p);
    } else {
        p.error("expected `{`");
    }
    m.complete(p, MATCH_EXPR)
}

fn match_arm_list(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T!['{']);
    attributes::inner_attrs(p);
    let arm_first = patterns::PATTERN_FIRST.union(TokenSet::new(&[T![#]]));
    while !p.at(T!['}']) && !p.at_end() {
        if !p.at_ts(arm_first) {
            p.err_and_bump("expected a match arm");
            continue;
        }
        match_arm(p);
    }
    p.expect(T!['}']);
    m.complete(p, MATCH_ARM_LIST);
}

fn match_arm(p: &mut Parser<'_>) {
    let m = p.start();
    attributes::outer_attrs(p);
    patterns::pattern(p);
    if p.at(T![if]) {
        let guard = p.start();
        p.bump(T![if]);
        expr(p);
        guard.complete(p, MATCH_GUARD);
    }
    p.expect(T![=>]);
    // Like a statement, a block-like body needs no comma.
    let blocklike = expr_bp(p, STMT, 1).map_or(BlockLike::NotBlock, |(_, b)| b);
    if !p.eat(T![,]) && blocklike == BlockLike::NotBlock && !p.at(T!['}']) {
        p.error("expected `,`");
    }
    m.complete(p, MATCH_ARM);
}
