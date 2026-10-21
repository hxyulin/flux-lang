//! Paths come in three flavors that differ only in how generic arguments
//! are written:
//!
//! - use paths (`a::b`, also attributes and visibilities) take none;
//! - type paths take `Vec<T>` directly, and `Fn(A) -> B`;
//! - expression paths need the turbofish, `Vec::<T>::new`, because a bare
//!   `<` there is less-than.

use super::*;

pub(super) const USE_PATH_FIRST: TokenSet = NAME_REF_FIRST.union(TokenSet::new(&[T![:]]));

/// Tokens that can start any path. `:` stands for a leading `::` and `<`
/// for a qualified path such as `<T as Trait>::Assoc`.
pub(super) const PATH_FIRST: TokenSet = USE_PATH_FIRST.union(TokenSet::new(&[T![<]]));

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Use,
    Type,
    Expr,
}

pub(super) fn is_path_start(p: &mut Parser<'_>) -> bool {
    p.at_ts(NAME_REF_FIRST) || p.at(T![::]) || p.at(T![<])
}

pub(super) fn is_use_path_start(p: &mut Parser<'_>) -> bool {
    p.at_ts(NAME_REF_FIRST) || p.at(T![::])
}

pub(super) fn use_path(p: &mut Parser<'_>) {
    path(p, Mode::Use);
}

pub(super) fn type_path(p: &mut Parser<'_>) {
    path(p, Mode::Type);
}

pub(super) fn expr_path(p: &mut Parser<'_>) {
    path(p, Mode::Expr);
}

/// Each `::` wraps the path so far, so `a::b::c` is
/// `PATH(PATH(PATH(a) :: b) :: c)`: the qualifier of every segment is a
/// path itself.
fn path(p: &mut Parser<'_>, mode: Mode) {
    let m = p.start();
    path_segment(p, mode, true);
    let mut qualifier = m.complete(p, PATH);
    loop {
        // In `use a::{b, c}` and `use a::*`, the `::` belongs to the tree.
        let ends_use_path = mode == Mode::Use && matches!(p.nth(2), T![*] | T!['{']);
        if !p.at(T![::]) || ends_use_path {
            break;
        }
        let m = qualifier.precede(p);
        p.bump(T![::]);
        path_segment(p, mode, false);
        qualifier = m.complete(p, PATH);
    }
}

fn path_segment(p: &mut Parser<'_>, mode: Mode, first: bool) {
    let m = p.start();
    if first && p.at(T![<]) {
        // `<T>::f` or `<T as Trait>::f`.
        p.bump(T![<]);
        types::type_(p);
        if p.eat(T![as]) {
            types::path_type(p);
        }
        p.expect(T![>]);
        if !p.at(T![::]) {
            p.error("expected `::`");
        }
        m.complete(p, PATH_SEGMENT);
        return;
    }
    if first {
        p.eat(T![::]);
    }
    name_ref(p);
    match mode {
        Mode::Use => {}
        Mode::Type => {
            if p.at(T![::]) && p.nth(2) == T![<] {
                p.bump(T![::]);
                generic_arg_list(p);
            } else if p.at(T![<]) && !p.at(T![<=]) {
                generic_arg_list(p);
            } else if p.at(T!['(']) {
                parenthesized_args(p);
                opt_ret_type(p, false);
            }
        }
        Mode::Expr => {
            if p.at(T![::]) && p.nth(2) == T![<] {
                p.bump(T![::]);
                generic_arg_list(p);
            }
        }
    }
    m.complete(p, PATH_SEGMENT);
}

/// `::<A, B>` after a method name, where only the turbofish form exists.
pub(super) fn opt_turbofish(p: &mut Parser<'_>) {
    if p.at(T![::]) && p.nth(2) == T![<] {
        p.bump(T![::]);
        generic_arg_list(p);
    }
}

/// `<T, 'a, 3, Item = U>`.
fn generic_arg_list(p: &mut Parser<'_>) {
    let list = p.start();
    let first = types::TYPE_FIRST.union(TokenSet::new(&[
        LIFETIME_IDENT,
        T!['{'],
        T![-],
        INT_NUMBER,
        FLOAT_NUMBER,
        CHAR,
        BYTE,
        STRING,
        T![true],
        T![false],
    ]));
    delimited(p, T![<], T![>], first, "a generic argument", generic_arg);
    list.complete(p, GENERIC_ARG_LIST);
}

fn generic_arg(p: &mut Parser<'_>) {
    let m = p.start();
    match p.current() {
        LIFETIME_IDENT => {
            lifetime(p);
            m.complete(p, LIFETIME_ARG);
        }
        // `Item = T`, `Item: Bound` and `Item<'a> = T`, but not `T::Item`,
        // `Vec<T>` or `a == b`.
        IDENT if at_assoc_type_arg(p) => {
            name_ref(p);
            if p.at(T![<]) {
                generic_arg_list(p);
            }
            if p.eat(T![=]) {
                types::type_(p);
            } else {
                p.bump(T![:]);
                generics::bounds(p);
            }
            m.complete(p, ASSOC_TYPE_ARG);
        }
        T!['{']
        | T![-]
        | INT_NUMBER
        | FLOAT_NUMBER
        | CHAR
        | BYTE
        | STRING
        | T![true]
        | T![false] => {
            m.abandon(p);
            const_arg(p);
        }
        _ => {
            types::type_(p);
            m.complete(p, TYPE_ARG);
        }
    }
}

fn at_assoc_type_arg(p: &mut Parser<'_>) -> bool {
    let after_name = if p.nth(1) == T![<] {
        match skip_angle_brackets(p, 1) {
            Some(n) => n,
            None => return false,
        }
    } else {
        1
    };
    match p.nth(after_name) {
        T![=] => !p.nth_at(after_name, T![==]),
        T![:] => !p.nth_at(after_name, T![::]),
        _ => false,
    }
}

/// Given `<` at lookahead `n`, the lookahead just past its matching `>`.
/// Gives up at tokens that cannot appear inside generic arguments.
fn skip_angle_brackets(p: &mut Parser<'_>, n: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = n;
    loop {
        match p.nth(i) {
            T![<] => depth += 1,
            // The `>` of `->` (in `Fn() -> T`) closes nothing.
            T![>] if i > n && p.nth_at(i - 1, T![->]) => {}
            T![>] => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            T![;] | T!['{'] | T!['}'] | EOF => return None,
            _ => {}
        }
        i += 1;
    }
}

/// A const generic argument: a literal, `-literal`, or a block.
pub(super) fn const_arg(p: &mut Parser<'_>) {
    let m = p.start();
    match p.current() {
        T!['{'] => expressions::block_expr(p),
        T![-] => {
            let neg = p.start();
            p.bump(T![-]);
            expressions::literal(p);
            neg.complete(p, PREFIX_EXPR);
        }
        _ if paths::is_path_start(p) => {
            let path = p.start();
            expr_path(p);
            path.complete(p, PATH_EXPR);
        }
        _ => expressions::literal(p),
    }
    m.complete(p, CONST_ARG);
}

/// `(A, B)` in `Fn(A, B) -> C`.
fn parenthesized_args(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T!['('],
        T![')'],
        types::TYPE_FIRST,
        "a type",
        types::type_,
    );
    list.complete(p, PARENTHESIZED_ARG_LIST);
}
