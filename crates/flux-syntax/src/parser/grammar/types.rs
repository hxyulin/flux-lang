use super::*;

pub(super) const TYPE_FIRST: TokenSet = paths::PATH_FIRST.union(TokenSet::new(&[
    T!['('],
    T!['['],
    T![!],
    T![*],
    T![&],
    T![_],
    T![fn],
    T![unsafe],
    T![extern],
    T![for],
    T![impl],
    T![dyn],
]));

pub(super) fn type_(p: &mut Parser<'_>) {
    type_inner(p, true);
}

/// A type where a following `+` is not part of it, as after `&`:
/// `&dyn A + B` is ambiguous and rejected, like rustc does.
pub(super) fn type_no_bounds(p: &mut Parser<'_>) {
    type_inner(p, false);
}

fn type_inner(p: &mut Parser<'_>, allow_bounds: bool) {
    match p.current() {
        T!['('] => paren_or_tuple_type(p),
        T![!] => {
            let m = p.start();
            p.bump(T![!]);
            m.complete(p, NEVER_TYPE);
        }
        T![*] => ptr_type(p),
        T!['['] => array_or_slice_type(p),
        T![&] => ref_type(p),
        T![_] => {
            let m = p.start();
            p.bump(T![_]);
            m.complete(p, INFER_TYPE);
        }
        T![fn] | T![unsafe] | T![extern] => fn_ptr_type(p),
        T![for] => {
            let m = p.start();
            generics::for_binder(p);
            type_no_bounds(p);
            m.complete(p, FOR_TYPE);
        }
        T![impl] => {
            let m = p.start();
            p.bump(T![impl]);
            bounds_for(p, allow_bounds);
            m.complete(p, IMPL_TRAIT_TYPE);
        }
        T![dyn] => {
            let m = p.start();
            p.bump(T![dyn]);
            bounds_for(p, allow_bounds);
            m.complete(p, DYN_TRAIT_TYPE);
        }
        _ if paths::is_path_start(p) => path_or_macro_type(p),
        _ => p.err_recover(
            "expected a type",
            TokenSet::new(&[T![,], T![;], T![=], T![>], T![')'], T![']']]),
        ),
    }
}

/// `impl A + B` takes several bounds only where a `+` cannot belong to an
/// enclosing type.
fn bounds_for(p: &mut Parser<'_>, allow_bounds: bool) {
    if allow_bounds {
        generics::bounds(p);
    } else {
        let list = p.start();
        if p.at_ts(paths::PATH_FIRST.union(TokenSet::new(&[LIFETIME_IDENT, T![?], T![for]]))) {
            // A single bound; reuse the list rule but stop before `+`.
            let bound = p.start();
            if p.at(LIFETIME_IDENT) {
                lifetime(p);
            } else {
                p.eat(T![?]);
                if p.at(T![for]) {
                    generics::for_binder(p);
                }
                path_type(p);
            }
            bound.complete(p, TYPE_BOUND);
        } else {
            p.error("expected a trait");
        }
        list.complete(p, TYPE_BOUND_LIST);
    }
}

pub(super) fn path_type(p: &mut Parser<'_>) {
    let m = p.start();
    paths::type_path(p);
    m.complete(p, PATH_TYPE);
}

fn path_or_macro_type(p: &mut Parser<'_>) {
    let m = p.start();
    paths::type_path(p);
    if p.at(T![!]) && !p.at(T![!=]) {
        p.bump(T![!]);
        if matches!(p.current(), T!['('] | T!['['] | T!['{']) {
            attributes::token_tree(p);
        } else {
            p.error("expected `(`, `[` or `{`");
        }
        m.complete(p, MACRO_CALL);
    } else {
        m.complete(p, PATH_TYPE);
    }
}

/// `()`, `(T)`, `(T,)` or `(A, B)`.
fn paren_or_tuple_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T!['(']);
    let mut count = 0;
    let mut trailing_comma = false;
    while !p.at(T![')']) && !p.at_end() {
        if !p.at_ts(TYPE_FIRST) {
            p.err_recover("expected a type", TokenSet::new(&[T![,], T![;]]));
            if !p.eat(T![,]) {
                break;
            }
            continue;
        }
        type_(p);
        count += 1;
        trailing_comma = p.eat(T![,]);
        if !trailing_comma {
            break;
        }
    }
    p.expect(T![')']);
    let kind = if count == 1 && !trailing_comma {
        PAREN_TYPE
    } else {
        TUPLE_TYPE
    };
    m.complete(p, kind);
}

/// `*const T` or `*mut T`.
fn ptr_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T![*]);
    if !p.eat(T![const]) && !p.eat(T![mut]) {
        p.error("expected `const` or `mut`");
    }
    type_no_bounds(p);
    m.complete(p, PTR_TYPE);
}

/// `[T]` or `[T; N]`.
fn array_or_slice_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T!['[']);
    type_(p);
    let kind = if p.eat(T![;]) {
        expressions::expr(p);
        ARRAY_TYPE
    } else {
        SLICE_TYPE
    };
    p.expect(T![']']);
    m.complete(p, kind);
}

/// `&T`, `&mut T`, `&'a T`.
fn ref_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.bump(T![&]);
    if p.at(LIFETIME_IDENT) {
        lifetime(p);
    }
    p.eat(T![mut]);
    type_no_bounds(p);
    m.complete(p, REF_TYPE);
}

/// `unsafe extern "C" fn(A, B) -> C`.
fn fn_ptr_type(p: &mut Parser<'_>) {
    let m = p.start();
    p.eat(T![unsafe]);
    if p.at(T![extern]) {
        abi(p);
    }
    if p.expect(T![fn]) {
        if p.at(T!['(']) {
            params::fn_ptr_param_list(p);
        } else {
            p.error("expected function parameters");
        }
    }
    opt_ret_type(p, false);
    m.complete(p, FN_PTR_TYPE);
}
