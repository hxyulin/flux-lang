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
        meta(p);
        p.expect(T![']']);
    } else {
        p.error("expected `[`");
    }
    m.complete(p, ATTR);
}

/// An attribute's contents: `path`, `path(tokens)` or `path = expr`.
pub(super) fn meta(p: &mut Parser<'_>) {
    let meta = p.start();
    // `#[unsafe(no_mangle)]`: the attribute asserts a safety invariant.
    let is_unsafe = p.at(T![unsafe]) && p.nth(1) == T!['('];
    if is_unsafe {
        p.bump(T![unsafe]);
        p.bump(T!['(']);
    }
    // Forwarded type fragments carry invisible parentheses. Attribute
    // paths use the path inside them, including `#[derive($trait)]` inputs.
    let mut wrappers = 0;
    while p.at(T!['(']) && p.input.is_invisible_paren(p.pos) {
        p.bump_any();
        wrappers += 1;
    }
    paths::use_path(p);
    match p.current() {
        T![=] => {
            p.bump(T![=]);
            expressions::expr(p);
        }
        T!['('] | T!['['] | T!['{'] => token_tree_inner(p, true),
        _ => {}
    }
    for _ in 0..wrappers {
        p.expect(T![')']);
    }
    if is_unsafe {
        p.expect(T![')']);
    }
    meta.complete(p, META);
}

/// A delimited group of arbitrary tokens, as in a macro call. Only the
/// delimiters must balance.
pub(super) fn token_tree(p: &mut Parser<'_>) {
    token_tree_inner(p, false);
}

fn token_tree_inner(p: &mut Parser<'_>, attribute: bool) {
    let close = match p.current() {
        T!['('] => T![')'],
        T!['['] => T![']'],
        T!['{'] => T!['}'],
        _ => unreachable!("token_tree() called without an opening delimiter"),
    };
    let invisible = attribute && p.input.is_invisible_paren(p.pos);
    let m = (!invisible).then(|| p.start());
    p.bump_any();
    while !p.at_end() && !p.at(close) {
        match p.current() {
            T!['('] | T!['['] | T!['{'] => token_tree_inner(p, attribute),
            // A stray `}` most likely closes an enclosing block.
            T!['}'] => break,
            T![')'] | T![']'] => p.err_and_bump("unmatched delimiter"),
            _ => p.bump_any(),
        }
    }
    p.expect(close);
    if let Some(m) = m {
        m.complete(p, TOKEN_TREE);
    }
}
