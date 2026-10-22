//! The flux grammar: currently a subset of Rust (edition 2024).
//!
//! Each function parses one construct starting at the current token. They
//! follow the same conventions:
//!
//! - A function named after a node always produces that node, reporting
//!   errors for missing parts rather than bailing out.
//! - An `opt_` function produces nothing when the construct is absent.
//! - Functions never loop without consuming a token; list loops either
//!   consume or break.

mod attributes;
mod expressions;
mod generics;
mod items;
mod params;
mod paths;
mod patterns;
mod types;

use super::{CompletedMarker, Fragment, Marker, Parser, TokenSet};
use crate::SyntaxKind::{self, *};
use crate::T;

pub(super) fn source_file(p: &mut Parser<'_>) {
    let m = p.start();
    attributes::inner_attrs(p);
    items::mod_contents(p, false);
    m.complete(p, SOURCE_FILE);
}

/// A macro expansion as a whole; see [`crate::parser::parse_expansion`].
pub(super) fn expansion(p: &mut Parser<'_>, kind: Fragment) {
    let m = p.start();
    let root = match kind {
        Fragment::Items => {
            items::mod_contents(p, false);
            MACRO_ITEMS
        }
        Fragment::Stmts => {
            while !p.at_end() {
                expressions::stmt(p);
            }
            MACRO_STMTS
        }
        _ => {
            fragment(p, kind);
            // rustc allows (and lints) a `;` after an expression.
            if kind == Fragment::Expr && p.at(T![;]) {
                p.bump(T![;]);
            }
            if !p.at_end() {
                let e = p.start();
                p.error(crate::parser::EXPANSION_IGNORES);
                while !p.at_end() {
                    p.bump_any();
                }
                e.complete(p, ERROR);
            }
            MACRO_FRAGMENT
        }
    };
    m.complete(p, root);
}

/// One fragment of kind `kind`, as a macro's matcher reads it.
pub(super) fn fragment(p: &mut Parser<'_>, kind: Fragment) {
    match kind {
        Fragment::Expr => {
            if p.at_ts(expressions::EXPR_FIRST) {
                expressions::expr(p);
            } else {
                p.error("expected an expression");
            }
        }
        Fragment::Pat => patterns::pattern(p),
        Fragment::PatParam => patterns::pattern_single(p),
        Fragment::Type => types::type_(p),
        Fragment::Path => {
            if paths::is_path_start(p) {
                paths::type_path(p);
            } else {
                p.error("expected a path");
            }
        }
        Fragment::Block => expressions::block_expr(p),
        Fragment::Stmt => expressions::stmt_fragment(p),
        Fragment::Item => {
            let m = p.start();
            attributes::outer_attrs(p);
            match items::opt_item(p, m) {
                Ok(()) => {}
                Err(m) => {
                    p.error("expected an item");
                    m.abandon(p);
                }
            }
        }
        Fragment::Vis => {
            opt_visibility(p);
        }
        Fragment::Lifetime => {
            if p.at(LIFETIME_IDENT) {
                lifetime(p);
            } else {
                p.error("expected a lifetime");
            }
        }
        Fragment::Literal => {
            p.eat(T![-]);
            if p.at_ts(expressions::LITERAL_FIRST) {
                expressions::literal(p);
            } else {
                p.error("expected a literal");
            }
        }
        Fragment::Meta => attributes::meta(p),
        Fragment::Items => items::mod_contents(p, false),
        Fragment::Stmts => {
            while !p.at_end() {
                expressions::stmt(p);
            }
        }
    }
}

fn name(p: &mut Parser<'_>) {
    name_r(p, TokenSet::EMPTY);
}

fn name_r(p: &mut Parser<'_>, recovery: TokenSet) {
    if p.at(IDENT) {
        let m = p.start();
        p.bump(IDENT);
        m.complete(p, NAME);
    } else if p.current().is_reserved_keyword() {
        let m = p.start();
        p.error("reserved keywords cannot be used as names");
        p.bump_any();
        m.complete(p, NAME);
    } else {
        p.err_recover("expected a name", recovery);
    }
}

const NAME_REF_FIRST: TokenSet = TokenSet::new(&[IDENT, SELF_KW, SUPER_KW, CRATE_KW, SELF_TYPE_KW]);

fn name_ref(p: &mut Parser<'_>) {
    if p.at_ts(NAME_REF_FIRST) {
        let m = p.start();
        p.bump_any();
        m.complete(p, NAME_REF);
    } else {
        p.err_recover("expected an identifier", TokenSet::EMPTY);
    }
}

/// A field name in `x.0` or `S { 0: x }`: a name or a tuple index.
fn name_ref_or_index(p: &mut Parser<'_>) {
    if p.at(INT_NUMBER) || p.at(IDENT) {
        let m = p.start();
        p.bump_any();
        m.complete(p, NAME_REF);
    } else {
        p.err_recover("expected a field name or tuple index", TokenSet::EMPTY);
    }
}

fn lifetime(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(LIFETIME_IDENT);
    m.complete(p, LIFETIME);
}

fn opt_visibility(p: &mut Parser<'_>) -> bool {
    if !p.at(T![pub]) {
        return false;
    }
    let m = p.start();
    p.bump(T![pub]);
    if p.at(T!['(']) {
        // `pub(crate)` and friends, but not the type in `struct S(pub (u8,));`.
        let restricted =
            matches!(p.nth(1), T![crate] | T![self] | T![super]) && p.nth(2) == T![')'];
        if restricted {
            p.bump(T!['(']);
            p.bump_any();
            p.bump(T![')']);
        } else if p.nth(1) == T![in] {
            p.bump(T!['(']);
            p.bump(T![in]);
            paths::use_path(p);
            p.expect(T![')']);
        }
    }
    m.complete(p, VISIBILITY);
    true
}

/// `-> Type`. In `Fn() -> T + Send` and `fn() -> T` pointer types the
/// type takes no bounds, because a `+` belongs to the enclosing bound list.
fn opt_ret_type(p: &mut Parser<'_>, allow_bounds: bool) -> bool {
    if !p.at(T![->]) {
        return false;
    }
    let m = p.start();
    p.bump(T![->]);
    if allow_bounds {
        types::type_(p);
    } else {
        types::type_no_bounds(p);
    }
    m.complete(p, RET_TYPE);
    true
}

/// `extern "C"`, or plain `extern`.
fn abi(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T![extern]);
    if p.at(STRING) {
        p.bump(STRING);
    }
    m.complete(p, ABI);
}

/// Parses `open (element sep)* close`, allowing a trailing separator.
/// `element` is called only when the current token is in `first`; anything
/// else is reported and skipped so the loop always makes progress.
fn delimited(
    p: &mut Parser<'_>,
    open: SyntaxKind,
    close: SyntaxKind,
    first: TokenSet,
    what: &str,
    mut element: impl FnMut(&mut Parser<'_>),
) {
    p.bump(open);
    while !p.at(close) && !p.at_end() {
        if !p.at_ts(first) {
            if p.at(T!['}']) || p.at(T![;]) {
                break;
            }
            p.err_and_bump(&format!("expected {what}"));
            continue;
        }
        element(p);
        if !p.at(close) && !p.eat(T![,]) {
            if p.at_ts(first) {
                p.error("expected `,`");
            } else {
                break;
            }
        }
    }
    p.expect(close);
}
