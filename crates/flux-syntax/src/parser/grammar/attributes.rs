use super::*;

pub(super) fn inner_attrs(p: &mut Parser<'_>) {
    while p.at(T![#]) && p.nth(1) == T![!] {
        attr(p, true);
    }
}

pub(super) fn outer_attrs(p: &mut Parser<'_>) {
    while p.at(T![#]) {
        attr(p, false);
    }
}

/// `#[path]`, `#[path(tokens)]` or `#[path = expr]`, with `#!` for inner
/// attributes.
fn attr(p: &mut Parser<'_>, inner: bool) {
    let m = p.start();
    p.bump(T![#]);
    if inner {
        p.bump(T![!]);
    }
    if p.eat(T!['[']) {
        let meta = p.start();
        // `#[unsafe(no_mangle)]`: the attribute asserts a safety invariant.
        let is_unsafe = p.at(T![unsafe]) && p.nth(1) == T!['('];
        if is_unsafe {
            p.bump(T![unsafe]);
            p.bump(T!['(']);
        }
        paths::use_path(p);
        match p.current() {
            T![=] => {
                p.bump(T![=]);
                expressions::expr(p);
            }
            T!['('] | T!['['] | T!['{'] => token_tree(p),
            _ => {}
        }
        if is_unsafe {
            p.expect(T![')']);
        }
        meta.complete(p, META);
        p.expect(T![']']);
    } else {
        p.error("expected `[`");
    }
    m.complete(p, ATTR);
}

/// A delimited group of arbitrary tokens, as in a macro call. Only the
/// delimiters must balance.
pub(super) fn token_tree(p: &mut Parser<'_>) {
    let close = match p.current() {
        T!['('] => T![')'],
        T!['['] => T![']'],
        T!['{'] => T!['}'],
        _ => unreachable!("token_tree() called without an opening delimiter"),
    };
    let m = p.start();
    p.bump_any();
    while !p.at_end() && !p.at(close) {
        match p.current() {
            T!['('] | T!['['] | T!['{'] => token_tree(p),
            // A stray `}` most likely closes an enclosing block.
            T!['}'] => break,
            T![')'] | T![']'] => p.err_and_bump("unmatched delimiter"),
            _ => p.bump_any(),
        }
    }
    p.expect(close);
    m.complete(p, TOKEN_TREE);
}
