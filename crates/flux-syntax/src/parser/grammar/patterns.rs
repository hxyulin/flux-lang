use super::*;

const LITERAL_PAT_FIRST: TokenSet = TokenSet::new(&[
    T![-],
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

pub(super) const PATTERN_FIRST: TokenSet =
    paths::PATH_FIRST
        .union(LITERAL_PAT_FIRST)
        .union(TokenSet::new(&[
            T![_],
            T![.],
            T![&],
            T!['('],
            T!['['],
            T![ref],
            T![mut],
            T![|],
        ]));

/// A pattern that may be an or-pattern, with an optional leading `|`:
/// `| A | B(x)`.
pub(super) fn pattern(p: &mut Parser<'_>) {
    let m = p.start();
    let leading_pipe = p.eat(T![|]);
    pattern_single(p);
    if !p.at(T![|]) || p.at(T![||]) {
        if leading_pipe {
            m.complete(p, OR_PAT);
        } else {
            m.abandon(p);
        }
        return;
    }
    while p.at(T![|]) && !p.at(T![||]) {
        p.bump(T![|]);
        pattern_single(p);
    }
    m.complete(p, OR_PAT);
}

/// A pattern without a top-level `|`, as in closure and function
/// parameters where `|` would be ambiguous.
pub(super) fn pattern_single(p: &mut Parser<'_>) {
    match p.current() {
        T![_] => {
            let m = p.start();
            p.bump(T![_]);
            m.complete(p, WILDCARD_PAT);
        }
        T![.] if p.at(T![..=]) || p.at(T![...]) || (p.at(T![..]) && at_range_end(p, 2)) => {
            // `..=5` or `..5`: a range with no start.
            let m = p.start();
            if !p.eat(T![..=]) && !p.eat(T![...]) {
                p.bump(T![..]);
            }
            range_end(p);
            m.complete(p, RANGE_PAT);
        }
        T![.] if p.at(T![..]) => {
            let m = p.start();
            p.bump(T![..]);
            m.complete(p, REST_PAT);
        }
        T![&] => {
            let m = p.start();
            p.bump(T![&]);
            p.eat(T![mut]);
            pattern_single(p);
            m.complete(p, REF_PAT);
        }
        T!['('] => tuple_or_paren_pat(p),
        T!['['] => {
            let m = p.start();
            pattern_list(p, T!['['], T![']']);
            m.complete(p, SLICE_PAT);
        }
        T![ref] | T![mut] => ident_pat(p),
        // A lone name binds; a name followed by `::`, `(`, `{`, `!` or a
        // range operator is a path.
        IDENT if !at_path_continuation(p) => ident_pat(p),
        // Reported by `name`, which explains that the word is reserved.
        kind if kind.is_reserved_keyword() => ident_pat(p),
        _ if p.at_ts(LITERAL_PAT_FIRST) => {
            let lhs = literal_pat(p);
            opt_range(p, lhs);
        }
        _ if paths::is_path_start(p) => path_pat(p),
        _ => p.err_recover(
            "expected a pattern",
            TokenSet::new(&[T![,], T![=], T![:], T![|], T![')'], T![']'], T![if], T![in]]),
        ),
    }
}

fn at_path_continuation(p: &mut Parser<'_>) -> bool {
    matches!(p.nth(1), T![:] | T!['('] | T!['{'] | T![!] | T![.])
        && (p.nth(1) != T![:] || p.nth_at(1, T![::]))
        && (p.nth(1) != T![.] || p.nth_at(1, T![..]))
}

/// `name`, `ref mut name`, `name @ subpattern`.
fn ident_pat(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(T![ref]);
    p.eat(T![mut]);
    name(p);
    if p.eat(T![@]) {
        pattern_single(p);
    }
    m.complete(p, IDENT_PAT);
}

fn literal_pat(p: &mut Parser<'_>) -> CompletedMarker {
    let m = p.start();
    if p.eat(T![-]) && !p.at(INT_NUMBER) && !p.at(FLOAT_NUMBER) {
        p.error("expected a number after `-`");
    }
    expressions::literal(p);
    m.complete(p, LITERAL_PAT)
}

fn path_pat(p: &mut Parser<'_>) {
    let m = p.start();
    paths::expr_path(p);
    match p.current() {
        T!['('] => {
            pattern_list(p, T!['('], T![')']);
            m.complete(p, TUPLE_STRUCT_PAT);
        }
        T!['{'] => {
            record_pat_field_list(p);
            m.complete(p, RECORD_PAT);
        }
        T![!] => {
            p.bump(T![!]);
            if matches!(p.current(), T!['('] | T!['['] | T!['{']) {
                attributes::token_tree(p);
            } else {
                p.error("expected `(`, `[` or `{`");
            }
            m.complete(p, MACRO_CALL);
        }
        _ => {
            let lhs = m.complete(p, PATH_PAT);
            opt_range(p, lhs);
        }
    }
}

/// Whether the token `n` ahead can end a range pattern.
fn at_range_end(p: &mut Parser<'_>, n: usize) -> bool {
    let kind = p.nth(n);
    LITERAL_PAT_FIRST.contains(kind) || NAME_REF_FIRST.contains(kind) || kind == T![<]
}

fn range_end(p: &mut Parser<'_>) {
    if p.at_ts(LITERAL_PAT_FIRST) {
        literal_pat(p);
    } else if paths::is_path_start(p) {
        let m = p.start();
        paths::expr_path(p);
        m.complete(p, PATH_PAT);
    } else {
        p.error("expected the end of the range");
    }
}

/// `lhs..=rhs`, `lhs..rhs` or `lhs..` after a literal or path pattern.
fn opt_range(p: &mut Parser<'_>, lhs: CompletedMarker) {
    if p.at(T![..=]) || p.at(T![...]) {
        let m = lhs.precede(p);
        if !p.eat(T![..=]) {
            p.bump(T![...]);
        }
        range_end(p);
        m.complete(p, RANGE_PAT);
    } else if p.at(T![..]) {
        let m = lhs.precede(p);
        p.bump(T![..]);
        if at_range_end(p, 0) {
            range_end(p);
        }
        m.complete(p, RANGE_PAT);
    }
}

/// `()`, `(p)`, `(p,)` or `(a, b, ..)`.
fn tuple_or_paren_pat(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T!['(']);
    let mut count = 0;
    let mut trailing_comma = false;
    let mut has_rest = false;
    while !p.at(T![')']) && !p.at_end() {
        if !p.at_ts(PATTERN_FIRST) {
            p.err_and_bump("expected a pattern");
            continue;
        }
        has_rest |= p.at(T![..]) && !at_range_end(p, 2);
        pattern(p);
        count += 1;
        trailing_comma = p.eat(T![,]);
        if !trailing_comma {
            break;
        }
    }
    p.expect(T![')']);
    let kind = if count == 1 && !trailing_comma && !has_rest {
        PAREN_PAT
    } else {
        TUPLE_PAT
    };
    m.complete(p, kind);
}

fn pattern_list(p: &mut Parser<'_>, open: SyntaxKind, close: SyntaxKind) {
    delimited(p, open, close, PATTERN_FIRST, "a pattern", pattern);
}

/// `{ a, b: pat, ref mut c, 0: x, .. }`.
fn record_pat_field_list(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T!['{'],
        T!['}'],
        TokenSet::new(&[IDENT, INT_NUMBER, T![ref], T![mut], T![.], T![#]]),
        "a field pattern",
        |p| {
            if p.at(T![..]) {
                let m = p.start();
                p.bump(T![..]);
                m.complete(p, REST_PAT);
                return;
            }
            let m = p.start();
            attributes::outer_attrs(p);
            if matches!(p.current(), IDENT | INT_NUMBER) && p.nth(1) == T![:] {
                name_ref_or_index(p);
                p.bump(T![:]);
                pattern(p);
            } else {
                ident_pat(p);
            }
            m.complete(p, RECORD_PAT_FIELD);
        },
    );
    list.complete(p, RECORD_PAT_FIELD_LIST);
}
