use super::*;

/// Tokens that start an item; error recovery stops before them.
pub(super) const ITEM_RECOVERY_SET: TokenSet = TokenSet::new(&[
    T![fn],
    T![struct],
    T![enum],
    T![impl],
    T![trait],
    T![const],
    T![static],
    T![type],
    T![mod],
    T![use],
    T![pub],
    T![unsafe],
    T![extern],
    T![;],
    T![#],
]);

/// Items until the end of input, or until `}` in a module body.
pub(super) fn mod_contents(p: &mut Parser<'_>, in_braces: bool) {
    while !p.at_end() && !(in_braces && p.at(T!['}'])) {
        item_or_macro(p, in_braces);
    }
}

pub(super) fn item_or_macro(p: &mut Parser<'_>, in_braces: bool) {
    let m = p.start();
    let has_attrs = p.at(T![#]);
    attributes::outer_attrs(p);
    let m = match opt_item(p, m) {
        Ok(()) => return,
        Err(m) => m,
    };
    if let Some(message) = missing_keyword(p, 0, false) {
        p.error(message);
        missing_keyword_item(p);
        m.complete(p, ERROR);
        return;
    }
    if paths::is_path_start(p) {
        macro_call_item(p, m);
        return;
    }
    if has_attrs {
        p.error("expected an item after attributes");
        m.complete(p, ERROR);
        return;
    }
    m.abandon(p);
    match p.current() {
        T!['{'] => {
            let m = p.start();
            p.error("expected an item");
            attributes::token_tree(p);
            m.complete(p, ERROR);
        }
        T!['}'] if !in_braces => {
            let m = p.start();
            p.error("unmatched `}`");
            p.bump(T!['}']);
            m.complete(p, ERROR);
        }
        T!['}'] => p.error("expected an item"),
        _ => p.err_and_bump("expected an item"),
    }
}

/// Parses an item into `m` if one starts here, after its attributes. On
/// `Err`, nothing was consumed.
pub(super) fn opt_item(p: &mut Parser<'_>, m: Marker) -> Result<(), Marker> {
    // rustc reports a missing keyword after `pub` at `pub`.
    if p.at(T![pub])
        && let Some(message) = missing_keyword(p, 1, true)
    {
        p.error(message);
        opt_visibility(p);
        missing_keyword_item(p);
        m.complete(p, ERROR);
        return Ok(());
    }
    let has_visibility = opt_visibility(p);
    if p.at(T![extern]) && p.nth(1) == T![crate] {
        extern_crate(p, m);
        return Ok(());
    }

    // Function qualifiers: `const unsafe extern "C" fn`. Each check looks
    // one token ahead so that `const X`, `unsafe { }`, `async move { }` and
    // friends stay available to their own rules.
    let mut has_mods = false;
    let mut has_abi = false;
    if p.at(T![const]) && !matches!(p.nth(1), IDENT | T![_] | T!['{']) {
        p.bump(T![const]);
        has_mods = true;
    }
    if p.at(T![async]) && !matches!(p.nth(1), T!['{'] | T![move] | T![|]) {
        p.bump(T![async]);
        has_mods = true;
    }
    if p.at(T![unsafe]) && p.nth(1) != T!['{'] {
        p.bump(T![unsafe]);
        has_mods = true;
    }
    if p.at(T![extern]) {
        abi(p);
        has_mods = true;
        has_abi = true;
    }

    match p.current() {
        T![fn] => fn_(p, m),
        T![impl] => impl_(p, m),
        T![trait] => trait_(p, m),
        T!['{'] if has_abi => extern_block(p, m),
        _ if has_mods => {
            p.error("expected `fn`, `impl` or `trait`");
            m.complete(p, ERROR);
        }
        IDENT if p.at_union() && p.nth(1) == IDENT => union_(p, m),
        T![struct] => struct_(p, m),
        T![enum] => enum_(p, m),
        T![type] => type_alias(p, m),
        T![mod] => module(p, m),
        T![use] => use_(p, m),
        T![const] if p.nth(1) != T!['{'] => const_or_static(p, m, T![const], CONST),
        T![static] => const_or_static(p, m, T![static], STATIC),
        _ if has_visibility => {
            if let Some(message) = missing_keyword(p, 0, false) {
                p.error(message);
                missing_keyword_item(p);
            } else {
                p.error("expected an item after the visibility");
            }
            m.complete(p, ERROR);
        }
        _ => return Err(m),
    }
    Ok(())
}

/// A name `at` tokens ahead where an item should start, followed by
/// what a function, struct or enum definition has after its name
/// (`name(..) { .. }`, `name { .. }`, `name<..>`): an item whose keyword
/// is missing, which rustc reports (`recover_missing_kw_before_item`)
/// with the message returned, telling what was meant from what follows.
/// `is_pub`: after `pub`.
fn missing_keyword(p: &mut Parser<'_>, at: usize, is_pub: bool) -> Option<&'static str> {
    if p.nth(at) != IDENT || !matches!(p.nth(at + 1), T!['('] | T!['{'] | T![<]) {
        return None;
    }
    // Look past the generic parameters (to the first `>`, as rustc
    // does) and the parentheses.
    let mut next = at + 1;
    let generics = p.nth(next) == T![<];
    if generics {
        while !matches!(p.nth(next), T![>] | T!['{'] | T![;] | EOF) {
            next += 1;
        }
        if p.nth(next) == T![>] {
            next += 1;
        }
    }
    let field = |p: &mut Parser<'_>, n: usize| p.nth_at(n, T![:]) && !p.nth_at(n, T![::]);
    let message = match p.nth(next) {
        T!['{'] if p.nth(next + 1) == T!['}'] => {
            "missing `enum` or `struct` for enum or struct definition"
        }
        T!['{'] if field(p, next + 2) || field(p, next + 3) => {
            "missing `struct` for struct definition"
        }
        T!['{'] => "missing `enum` for enum definition",
        T!['('] => {
            let method = matches!(
                (p.nth(next + 1), p.nth(next + 2), p.nth(next + 3)),
                (T![self], _, _)
                    | (T![&] | T![mut], T![self], _)
                    | (T![&], T![mut] | LIFETIME_IDENT, T![self])
            );
            let mut close = next;
            let mut depth = 0u32;
            loop {
                match p.nth(close) {
                    T!['('] => depth += 1,
                    T![')'] if depth == 1 => break,
                    T![')'] => depth -= 1,
                    EOF => break,
                    _ => {}
                }
                close += 1;
            }
            if p.nth_at(close + 1, T![->]) || p.nth(close + 1) == T!['{'] {
                if method {
                    "missing `fn` for method definition"
                } else {
                    "missing `fn` for function definition"
                }
            } else if is_pub && p.nth(close + 1) == T![;] {
                "missing `struct` for struct definition"
            } else {
                "missing `fn` or `struct` for function or struct definition"
            }
        }
        _ if generics => "missing `fn` or `struct` for function or struct definition",
        _ => return None,
    };
    Some(message)
}

/// The item [`missing_keyword`] found, from its name: what it would have
/// defined, or the call it is, as rustc skips it.
fn missing_keyword_item(p: &mut Parser<'_>) {
    p.bump(IDENT);
    if p.eat(T![<]) {
        while !p.at_end() && !matches!(p.current(), T![>] | T!['{'] | T![;]) {
            p.bump_any();
        }
        p.eat(T![>]);
    }
    match p.current() {
        T!['{'] => attributes::token_tree(p),
        T!['('] => {
            attributes::token_tree(p);
            if p.at(T![->]) || p.at(T!['{']) {
                while !p.at_end() && !matches!(p.current(), T!['{'] | T![;] | T!['}']) {
                    p.bump_any();
                }
                if p.at(T!['{']) {
                    attributes::token_tree(p);
                }
            } else {
                p.eat(T![;]);
            }
        }
        _ => {}
    }
}

/// `fn name<T>(params) -> Ret where ... { body }`, or `;` for a
/// declaration without a body.
fn fn_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![fn]);
    name_r(p, TokenSet::new(&[T!['('], T![<]]));
    generics::opt_generic_param_list(p);
    if p.at(T!['(']) {
        params::fn_param_list(p);
    } else {
        p.error("expected function parameters");
    }
    opt_ret_type(p, true);
    generics::opt_where_clause(p);
    if !p.eat(T![;]) {
        if p.at(T!['{']) {
            expressions::block_expr(p);
        } else {
            p.error("expected a function body or `;`");
        }
    }
    m.complete(p, FN);
}

fn struct_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![struct]);
    name_r(p, ITEM_RECOVERY_SET);
    generics::opt_generic_param_list(p);
    match p.current() {
        T!['('] => {
            tuple_field_list(p);
            generics::opt_where_clause(p);
            p.expect(T![;]);
        }
        _ => {
            generics::opt_where_clause(p);
            match p.current() {
                T![;] => p.bump(T![;]),
                T!['{'] => record_field_list(p),
                _ => p.error("expected `;`, `{` or `(`"),
            }
        }
    }
    m.complete(p, STRUCT);
}

/// `union` is contextual: `union! {}`, `fn union()` and paths named
/// `union` remain identifiers. A declaration has a following name.
fn union_(p: &mut Parser<'_>, m: Marker) {
    p.bump_union();
    name_r(p, ITEM_RECOVERY_SET);
    generics::opt_generic_param_list(p);
    generics::opt_where_clause(p);
    if p.at(T!['{']) {
        record_field_list(p);
    } else {
        p.error("expected `{`");
    }
    m.complete(p, UNION);
}

fn enum_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![enum]);
    name_r(p, ITEM_RECOVERY_SET);
    generics::opt_generic_param_list(p);
    generics::opt_where_clause(p);
    if p.at(T!['{']) {
        let list = p.start();
        delimited(
            p,
            T!['{'],
            T!['}'],
            TokenSet::new(&[IDENT, T![#], T![pub]]),
            "a variant",
            |p| {
                let m = p.start();
                attributes::outer_attrs(p);
                // Not allowed, but parsed, as rustc does, to say so.
                opt_visibility(p);
                name(p);
                match p.current() {
                    T!['{'] => record_field_list(p),
                    T!['('] => tuple_field_list(p),
                    _ => {}
                }
                if p.eat(T![=]) {
                    expressions::expr(p);
                }
                m.complete(p, VARIANT);
            },
        );
        list.complete(p, VARIANT_LIST);
    } else {
        p.error("expected `{`");
    }
    m.complete(p, ENUM);
}

pub(super) fn record_field_list(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T!['{'],
        T!['}'],
        TokenSet::new(&[IDENT, T![#], T![pub]]),
        "a field",
        |p| {
            let m = p.start();
            attributes::outer_attrs(p);
            opt_visibility(p);
            name(p);
            p.expect(T![:]);
            types::type_(p);
            m.complete(p, RECORD_FIELD);
        },
    );
    list.complete(p, RECORD_FIELD_LIST);
}

fn tuple_field_list(p: &mut Parser<'_>) {
    let list = p.start();
    delimited(
        p,
        T!['('],
        T![')'],
        types::TYPE_FIRST.union(TokenSet::new(&[T![#], T![pub]])),
        "a field type",
        |p| {
            let m = p.start();
            attributes::outer_attrs(p);
            opt_visibility(p);
            types::type_(p);
            m.complete(p, TUPLE_FIELD);
        },
    );
    list.complete(p, TUPLE_FIELD_LIST);
}

/// `trait Name<T>: Bounds where ... { items }`, after any `unsafe`.
fn trait_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![trait]);
    name_r(p, ITEM_RECOVERY_SET);
    generics::opt_generic_param_list(p);
    if p.eat(T![:]) {
        generics::bounds(p);
    }
    generics::opt_where_clause(p);
    assoc_item_list(p);
    m.complete(p, TRAIT);
}

/// `impl<T> Type { }` or `impl<T> Trait for Type { }`.
fn impl_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![impl]);
    if p.at(T![<]) && !generics::at_qualified_path(p) {
        generics::generic_param_list(p);
    }
    if p.at(T![!]) {
        p.err_and_bump("negative impls are not supported");
    }
    types::type_(p);
    if p.eat(T![for]) {
        types::type_(p);
    }
    generics::opt_where_clause(p);
    assoc_item_list(p);
    m.complete(p, IMPL);
}

fn assoc_item_list(p: &mut Parser<'_>) {
    if !p.at(T!['{']) {
        p.error("expected `{`");
        return;
    }
    let m = p.start();
    p.bump(T!['{']);
    attributes::inner_attrs(p);
    while !p.at(T!['}']) && !p.at_end() {
        item_or_macro(p, true);
    }
    p.expect(T!['}']);
    m.complete(p, ASSOC_ITEM_LIST);
}

/// `type Name<T>: Bounds where ... = Type;`; the bounds and value are
/// optional so the same rule serves traits and impls.
fn type_alias(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![type]);
    name_r(p, ITEM_RECOVERY_SET);
    generics::opt_generic_param_list(p);
    if p.eat(T![:]) {
        generics::bounds(p);
    }
    generics::opt_where_clause(p);
    if p.eat(T![=]) {
        types::type_(p);
        // The preferred place for a generic associated type's where clause.
        generics::opt_where_clause(p);
    }
    p.expect(T![;]);
    m.complete(p, TYPE_ALIAS);
}

/// `mod name;` or `mod name { items }`.
fn module(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![mod]);
    name_r(p, ITEM_RECOVERY_SET);
    if p.at(T!['{']) {
        let list = p.start();
        p.bump(T!['{']);
        attributes::inner_attrs(p);
        mod_contents(p, true);
        p.expect(T!['}']);
        list.complete(p, ITEM_LIST);
    } else {
        p.expect(T![;]);
    }
    m.complete(p, MODULE);
}

/// `const NAME: Type = value;` and `static mut NAME: Type = value;`. The
/// type and value are optional so that trait items parse.
fn const_or_static(p: &mut Parser<'_>, m: Marker, keyword: SyntaxKind, kind: SyntaxKind) {
    p.bump(keyword);
    if kind == STATIC {
        p.eat(T![mut]);
    }
    if kind == CONST && p.at(T![_]) {
        p.bump(T![_]);
    } else {
        name_r(p, ITEM_RECOVERY_SET);
    }
    if p.eat(T![:]) {
        types::type_(p);
    } else {
        p.error("expected `:` and a type");
    }
    if p.eat(T![=]) {
        expressions::expr(p);
    }
    p.expect(T![;]);
    m.complete(p, kind);
}

/// `extern crate name;` or `extern crate name as alias;`.
fn extern_crate(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![extern]);
    p.bump(T![crate]);
    name_ref(p);
    if p.at(T![as]) {
        let rename = p.start();
        p.bump(T![as]);
        if !p.eat(T![_]) {
            name(p);
        }
        rename.complete(p, RENAME);
    }
    p.expect(T![;]);
    m.complete(p, EXTERN_CRATE);
}

/// `unsafe extern "C" { fn f(); static X: i32; }`, after the ABI.
fn extern_block(p: &mut Parser<'_>, m: Marker) {
    let list = p.start();
    p.bump(T!['{']);
    attributes::inner_attrs(p);
    while !p.at(T!['}']) && !p.at_end() {
        item_or_macro(p, true);
    }
    p.expect(T!['}']);
    list.complete(p, EXTERN_ITEM_LIST);
    m.complete(p, EXTERN_BLOCK);
}

fn use_(p: &mut Parser<'_>, m: Marker) {
    p.bump(T![use]);
    use_tree(p);
    p.expect(T![;]);
    m.complete(p, USE);
}

/// `a::b`, `a::b as c`, `a::*`, `a::{b, c::d}`, `::{a}` or `*`.
fn use_tree(p: &mut Parser<'_>) {
    let m = p.start();
    match p.current() {
        T![*] => p.bump(T![*]),
        T!['{'] => use_tree_list(p),
        T![:] if p.at(T![::]) && matches!(p.nth(2), T![*] | T!['{']) => {
            p.bump(T![::]);
            if p.at(T![*]) {
                p.bump(T![*]);
            } else {
                use_tree_list(p);
            }
        }
        _ if paths::is_use_path_start(p) => {
            paths::use_path(p);
            if p.at(T![as]) {
                let rename = p.start();
                p.bump(T![as]);
                if !p.eat(T![_]) {
                    name(p);
                }
                rename.complete(p, RENAME);
            } else if p.eat(T![::]) {
                match p.current() {
                    T![*] => p.bump(T![*]),
                    T!['{'] => use_tree_list(p),
                    _ => p.error("expected `*` or `{`"),
                }
            }
        }
        _ => {
            m.abandon(p);
            p.err_recover("expected a use path", ITEM_RECOVERY_SET);
            return;
        }
    }
    m.complete(p, USE_TREE);
}

fn use_tree_list(p: &mut Parser<'_>) {
    let m = p.start();
    delimited(
        p,
        T!['{'],
        T!['}'],
        paths::USE_PATH_FIRST.union(TokenSet::new(&[T![*], T!['{']])),
        "a use path",
        use_tree,
    );
    m.complete(p, USE_TREE_LIST);
}

/// `path!(tokens);`, `path! { tokens }` or `macro_rules! name { rules }`.
fn macro_call_item(p: &mut Parser<'_>, m: Marker) {
    paths::use_path(p);
    if !p.expect(T![!]) {
        m.complete(p, ERROR);
        return;
    }
    // `macro_rules! name`: the defined macro's name sits between `!` and
    // the body.
    if p.at(IDENT) {
        name(p);
    }
    match p.current() {
        T!['{'] => attributes::token_tree(p),
        T!['('] | T!['['] => {
            attributes::token_tree(p);
            p.expect(T![;]);
        }
        _ => p.error("expected `(`, `[` or `{`"),
    }
    m.complete(p, MACRO_CALL);
}
