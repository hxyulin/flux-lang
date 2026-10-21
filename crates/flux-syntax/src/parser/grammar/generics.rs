use super::*;

pub(super) fn opt_generic_param_list(p: &mut Parser<'_>) {
    if p.at(T![<]) {
        generic_param_list(p);
    }
}

/// Whether `<` after `impl` starts a qualified path type (`impl <T as
/// Trait>::Assoc {}`) rather than generic parameters (`impl<T> ...`).
pub(super) fn at_qualified_path(p: &mut Parser<'_>) -> bool {
    match p.nth(1) {
        T![>] | T![#] | LIFETIME_IDENT | T![const] => false,
        IDENT => !matches!(p.nth(2), T![>] | T![,] | T![:] | T![=]),
        _ => true,
    }
}

/// `<'a, T: Bound = Default, const N: usize>`.
pub(super) fn generic_param_list(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T![<],
        T![>],
        TokenSet::new(&[IDENT, LIFETIME_IDENT, T![const], T![#]]),
        "a generic parameter",
        |p| {
            let m = p.start();
            attributes::outer_attrs(p);
            match p.current() {
                LIFETIME_IDENT => {
                    lifetime(p);
                    if p.eat(T![:]) {
                        lifetime_bounds(p);
                    }
                    m.complete(p, LIFETIME_PARAM);
                }
                T![const] => {
                    p.bump(T![const]);
                    name(p);
                    p.expect(T![:]);
                    types::type_(p);
                    if p.eat(T![=]) {
                        paths::const_arg(p);
                    }
                    m.complete(p, CONST_PARAM);
                }
                _ => {
                    name(p);
                    if p.eat(T![:]) {
                        bounds(p);
                    }
                    if p.eat(T![=]) {
                        types::type_(p);
                    }
                    m.complete(p, TYPE_PARAM);
                }
            }
        },
    );
    list.complete(p, GENERIC_PARAM_LIST);
}

/// `'a + 'b` after `'c:`.
fn lifetime_bounds(p: &mut Parser<'_>) {
    let list = p.start();
    while p.at(LIFETIME_IDENT) {
        let m = p.start();
        lifetime(p);
        m.complete(p, TYPE_BOUND);
        if !p.eat(T![+]) {
            break;
        }
    }
    list.complete(p, TYPE_BOUND_LIST);
}

const BOUND_FIRST: TokenSet = paths::PATH_FIRST.union(TokenSet::new(&[
    LIFETIME_IDENT,
    T![?],
    T![for],
    T!['('],
    T![use],
]));

/// `Trait + ?Sized + 'a + for<'b> Fn(&'b T)`. May be empty, as in `T:`.
pub(super) fn bounds(p: &mut Parser<'_>) {
    let list = p.start();
    while p.at_ts(BOUND_FIRST) {
        type_bound(p);
        if !p.eat(T![+]) {
            break;
        }
    }
    list.complete(p, TYPE_BOUND_LIST);
}

fn type_bound(p: &mut Parser<'_>) {
    let m = p.start();
    let parenthesized = p.eat(T!['(']);
    match p.current() {
        LIFETIME_IDENT => lifetime(p),
        T![use] => use_bound_generic_args(p),
        _ => {
            p.eat(T![?]);
            if p.at(T![for]) {
                for_binder(p);
            }
            if paths::is_path_start(p) {
                types::path_type(p);
            } else {
                p.error("expected a trait or lifetime");
            }
        }
    }
    if parenthesized {
        p.expect(T![')']);
    }
    m.complete(p, TYPE_BOUND);
}

/// `use<'a, T>`: the generic parameters an `impl Trait` may capture.
fn use_bound_generic_args(p: &mut Parser<'_>) {
    p.bump(T![use]);
    if !p.at(T![<]) {
        p.error("expected `<`");
        return;
    }
    let m = p.start();
    delimited(
        p,
        T![<],
        T![>],
        TokenSet::new(&[LIFETIME_IDENT, IDENT, T![Self]]),
        "a lifetime or type parameter",
        |p| {
            if p.at(LIFETIME_IDENT) {
                lifetime(p);
            } else {
                name_ref(p);
            }
        },
    );
    m.complete(p, USE_BOUND_GENERIC_ARGS);
}

/// `for<'a>`, for higher-ranked bounds and types.
pub(super) fn for_binder(p: &mut Parser<'_>) {
    p.bump(T![for]);
    if p.at(T![<]) {
        generic_param_list(p);
    } else {
        p.error("expected `<`");
    }
}

pub(super) fn opt_where_clause(p: &mut Parser<'_>) {
    if !p.at(T![where]) {
        return;
    }
    let m = p.start();
    p.bump(T![where]);
    let pred_first = types::TYPE_FIRST.union(TokenSet::new(&[LIFETIME_IDENT, T![for]]));
    while p.at_ts(pred_first) {
        where_predicate(p);
        if !p.eat(T![,]) {
            break;
        }
    }
    m.complete(p, WHERE_CLAUSE);
}

/// `T: Bound`, `'a: 'b`, or `for<'a> &'a T: Bound`.
fn where_predicate(p: &mut Parser<'_>) {
    let m = p.start();
    if p.at(LIFETIME_IDENT) {
        lifetime(p);
        p.expect(T![:]);
        lifetime_bounds(p);
    } else {
        if p.at(T![for]) {
            for_binder(p);
        }
        types::type_(p);
        p.expect(T![:]);
        bounds(p);
    }
    m.complete(p, WHERE_PRED);
}
