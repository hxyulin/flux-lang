use super::*;

/// `(self, a: A, b: B)` for a function or method, or `(fmt: *const u8, ...)`
/// for a C-variadic foreign function.
pub(super) fn fn_param_list(p: &mut Parser<'_>) {
    let list = p.start();
    p.bump(T!['(']);
    let mut first = true;
    while !p.at(T![')']) && !p.at_end() {
        if !p.at_ts(patterns::PATTERN_FIRST.union(TokenSet::new(&[T![#]]))) {
            if p.at(T!['{']) || p.at(T![;]) {
                break;
            }
            p.err_and_bump("expected a parameter");
            continue;
        }
        let m = p.start();
        attributes::outer_attrs(p);
        if first && at_self_param(p) {
            self_param(p, m);
        } else if p.eat(T![...]) {
            m.complete(p, PARAM);
        } else {
            patterns::pattern_single(p);
            p.expect(T![:]);
            if !p.eat(T![...]) {
                types::type_(p);
            }
            m.complete(p, PARAM);
        }
        first = false;
        if !p.at(T![')']) && !p.expect(T![,]) {
            break;
        }
    }
    p.expect(T![')']);
    list.complete(p, PARAM_LIST);
}

/// `self`, `mut self`, `&self`, `&'a mut self` or `self: Type`.
fn at_self_param(p: &mut Parser<'_>) -> bool {
    let after_ref = match p.current() {
        T![&] if p.nth(1) == LIFETIME_IDENT => 2,
        T![&] => 1,
        _ => 0,
    };
    let after_mut = after_ref + usize::from(p.nth(after_ref) == T![mut]);
    p.nth(after_mut) == T![self] && !p.nth_at(after_mut + 1, T![::])
}

fn self_param(p: &mut Parser<'_>, m: Marker) {
    let is_ref = p.eat(T![&]);
    if is_ref && p.at(LIFETIME_IDENT) {
        lifetime(p);
    }
    p.eat(T![mut]);
    p.bump(T![self]);
    if !is_ref && p.eat(T![:]) {
        types::type_(p);
    }
    m.complete(p, SELF_PARAM);
}

/// `(A, B)` or `(a: A, b: B)` in a function pointer type.
pub(super) fn fn_ptr_param_list(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T!['('],
        T![')'],
        types::TYPE_FIRST.union(TokenSet::new(&[T![#]])),
        "a parameter type",
        |p| {
            let m = p.start();
            attributes::outer_attrs(p);
            let named =
                matches!(p.current(), IDENT | T![_]) && p.nth(1) == T![:] && !p.nth_at(1, T![::]);
            if named {
                patterns::pattern_single(p);
                p.bump(T![:]);
            }
            types::type_(p);
            m.complete(p, PARAM);
        },
    );
    list.complete(p, PARAM_LIST);
}

/// `|a, b: B|` or `||` for a closure.
pub(super) fn closure_param_list(p: &mut Parser<'_>) {
    let list = p.start();
    if p.eat(T![||]) {
        list.complete(p, PARAM_LIST);
        return;
    }
    p.bump(T![|]);
    while !p.at(T![|]) && !p.at_end() {
        if !p.at_ts(patterns::PATTERN_FIRST.union(TokenSet::new(&[T![#]]))) {
            p.err_recover("expected a closure parameter", TokenSet::EMPTY);
            break;
        }
        let m = p.start();
        attributes::outer_attrs(p);
        patterns::pattern_single(p);
        if p.eat(T![:]) {
            types::type_(p);
        }
        m.complete(p, PARAM);
        if !p.at(T![|]) && !p.expect(T![,]) {
            break;
        }
    }
    p.expect(T![|]);
    list.complete(p, PARAM_LIST);
}
