//! Attribute admission and representation hints.
use super::{
    AstChildren, AstNode, Diagnostic, REPR_INTS, Symbol, SyntaxElement, SyntaxKind, T, TextRange,
    ast, cfg_position, macros, unsupported,
};

/// `non_exhaustive` is a safe, bare attribute; lists and values are errors.
pub(super) fn non_exhaustive_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) -> bool {
    let valid = attr.meta().is_some_and(|m| {
        m.token_tree().is_none() && m.eq_token().is_none() && m.unsafe_token().is_none()
    });
    if !valid {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "malformed `non_exhaustive` attribute input",
        ));
    }
    valid
}

/// Checks attributes not handled by a specialized semantic consumer.
pub(crate) fn check_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let attrs = crate::cfg::Attrs::new(attrs);
    diagnostics.extend(
        attrs
            .errors
            .iter()
            .map(|(r, m)| Diagnostic::new(*r, m.clone())),
    );
    for attr in attrs.iter() {
        check_attr(attr, diagnostics);
    }
}

/// A file's or module's own attributes: like [`check_attrs`], and in
/// `core` also the crate attributes rustc needs to compile it, which mean
/// nothing to flux.
pub(super) fn check_inner_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    is_core: bool,
    crate_root: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let attrs = crate::cfg::Attrs::new(attrs);
    diagnostics.extend(
        attrs
            .errors
            .iter()
            .map(|(r, m)| Diagnostic::new(*r, m.clone())),
    );
    for attr in attrs.iter() {
        if crate_root && attr_name(attr) == Some("no_std") {
            if !attr.meta().is_some_and(|m| {
                m.token_tree().is_none() && m.eq_token().is_none() && m.unsafe_token().is_none()
            }) {
                diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    "malformed `no_std` attribute input",
                ));
            }
            continue;
        }
        if !(is_core && attr_name(attr).is_some_and(|n| ["no_core", "feature"].contains(&n))) {
            check_attr(attr, diagnostics);
        }
    }
}

/// Only guaranteed baseline NEON on explicitly unsafe functions is supported.
/// No per-function ISA or safety metadata is needed: codegen already emits this
/// baseline, and ordinary unsafe function items retain all call/coercion rules.
pub(super) fn target_feature_attr(
    attr: ast::Attr<'_>,
    function: ast::Fn<'_>,
    in_extern_block: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let range = attr.syntax().text_range();
    let meta = attr.meta().expect("named attribute");
    if std::env::consts::ARCH != "aarch64" {
        unsupported(
            diagnostics,
            range,
            "`target_feature` attributes on this architecture",
        );
    }
    if meta.unsafe_token().is_some() {
        diagnostics.push(Diagnostic::new(
            range,
            "`target_feature` is not an unsafe attribute",
        ));
        return;
    }
    if in_extern_block || function.body().is_none() {
        diagnostics.push(Diagnostic::new(
            range,
            "`target_feature` requires a function with a body",
        ));
    }
    if function.unsafe_token().is_none() {
        unsupported(diagnostics, range, "`target_feature` on safe functions");
    }
    let mut tokens = Vec::new();
    if let Some(tree) = meta.token_tree() {
        macros::tt::tokens_of(tree.syntax(), &mut tokens);
    }
    let malformed = |diagnostics: &mut Vec<Diagnostic>| {
        diagnostics.push(Diagnostic::new(
            range,
            "expected `target_feature(enable = \"neon\")`",
        ));
    };
    let [open, inner @ .., close] = tokens.as_slice() else {
        malformed(diagnostics);
        return;
    };
    if meta.eq_token().is_some() || open.kind != T!['('] || close.kind != T![')'] {
        malformed(diagnostics);
        return;
    }
    let mut rest = inner;
    // Empty lists are inert in rustc. Repeated keys, attributes and features
    // are accepted too; validate each rather than silently dropping duplicates.
    while !rest.is_empty() {
        let [key, eq, value, tail @ ..] = rest else {
            malformed(diagnostics);
            return;
        };
        if key.text.as_str() != "enable" || eq.kind != T![=] || value.kind != SyntaxKind::STRING {
            malformed(diagnostics);
            return;
        }
        match crate::literal::unescape_str(value.text.as_str()) {
            Ok(features) => {
                for feature in features.split(',') {
                    if feature != "neon" {
                        diagnostics.push(Diagnostic::new(
                            value.range(),
                            format!("target feature `{feature}` is not supported yet"),
                        ));
                    }
                }
            }
            Err(message) => diagnostics.push(Diagnostic::new(value.range(), message)),
        }
        match tail {
            [] => break,
            [comma, tail @ ..] if comma.kind == T![,] => rest = tail,
            _ => {
                malformed(diagnostics);
                return;
            }
        }
    }
}

pub(super) fn target_feature_inline(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let meta = attr.meta().expect("named attribute");
    let mut tokens = Vec::new();
    if let Some(tree) = meta.token_tree() {
        macros::tt::tokens_of(tree.syntax(), &mut tokens);
    }
    let valid = if meta.unsafe_token().is_some() || meta.eq_token().is_some() {
        false
    } else if meta.token_tree().is_none() {
        true
    } else {
        let argument = match tokens.as_slice() {
            [open, arg, close] if open.kind == T!['('] && close.kind == T![')'] => Some(arg),
            [open, arg, comma, close]
                if open.kind == T!['('] && comma.kind == T![,] && close.kind == T![')'] =>
            {
                Some(arg)
            }
            _ => None,
        };
        match argument.map(|t| t.text.as_str()) {
            Some("always") => {
                diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    "cannot use `inline(always)` with `target_feature`",
                ));
                return;
            }
            Some("never") => true,
            _ => false,
        }
    };
    if !valid {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "malformed `inline` attribute input",
        ));
    }
}

pub(crate) fn attr_name(attr: ast::Attr<'_>) -> Option<&str> {
    attr.meta()
        .and_then(|m| m.path())
        .filter(|p| p.qualifier().is_none())
        .and_then(|p| p.segment())
        .and_then(|s| s.name_ref())
        .map(|n| n.text())
}

/// [`check_attrs`] for fields and parameters, where rustc rejects
/// `#[derive]` as an attribute macro rather than a misplaced derive.
pub(super) fn check_inert_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let attrs = crate::cfg::Attrs::new(attrs);
    diagnostics.extend(
        attrs
            .errors
            .iter()
            .map(|(r, m)| Diagnostic::new(*r, m.clone())),
    );
    for attr in attrs.iter() {
        let path = attr.meta().and_then(|m| m.path());
        match path {
            Some(path) if attr_name(attr) == Some("derive") => diagnostics.push(Diagnostic::new(
                path.syntax().text_range(),
                "expected non-macro attribute, found attribute macro `derive`",
            )),
            _ => check_attr(attr, diagnostics),
        }
    }
}

pub(crate) fn check_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    const IGNORED: &[&str] = &[
        "allow", "warn", "deny", "expect", "inline", "cold", "must_use", "doc",
    ];
    let name = attr_name(attr);
    if name == Some("deprecated") {
        deprecated_attr(attr, diagnostics);
        return;
    }
    if let Some(meta) = attr.meta()
        && meta.unsafe_token().is_none()
        && meta.token_tree().is_none()
        && meta.eq_token().is_none()
        && let Some(path) = meta.path()
        && path
            .segment()
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "skip")
        && let Some(tool) = path.qualifier()
        && tool.qualifier().is_none()
        && tool
            .segment()
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "rustfmt")
    {
        return;
    }
    if name == Some("cfg")
        && attr.meta().is_some_and(|m| m.unsafe_token().is_none())
        && attr.syntax().parent().is_some_and(cfg_position)
    {
        return;
    }
    if name.is_some_and(|n| IGNORED.contains(&n)) {
        return;
    }
    if name == Some("derive") {
        diagnostics.push(Diagnostic::new(
            attr.syntax().text_range(),
            "`derive` may only be applied to `struct`s, `enum`s and `union`s",
        ));
        return;
    }
    if name == Some("non_exhaustive") {
        if non_exhaustive_attr(attr, diagnostics) {
            let variant = attr
                .syntax()
                .parent()
                .is_some_and(|p| p.kind() == SyntaxKind::VARIANT);
            let message = if variant {
                "non-exhaustive enum variants are not supported yet"
            } else {
                "`non_exhaustive` may only be applied to structs, enums and enum variants"
            };
            diagnostics.push(Diagnostic::new(attr.syntax().text_range(), message));
        }
        return;
    }
    let message = match name {
        Some(name) => format!("the `{name}` attribute is not supported yet"),
        None => "attributes like this are not supported yet".to_owned(),
    };
    diagnostics.push(Diagnostic::new(attr.syntax().text_range(), message));
}

/// Deprecation is lint metadata; validate its payload as for other accepted
/// lint attributes. Flux does not currently emit deprecation warnings.
fn deprecated_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    let meta = attr.meta().expect("named attribute");
    let range = attr.syntax().text_range();
    let malformed = |diagnostics: &mut Vec<Diagnostic>| {
        diagnostics.push(Diagnostic::new(
            range,
            "malformed `deprecated` attribute input",
        ));
    };
    if meta.unsafe_token().is_some() {
        malformed(diagnostics);
        return;
    }
    if meta.eq_token().is_some() {
        if !matches!(meta.expr(), Some(ast::Expr::Literal(literal)) if literal.value().is_some_and(|token| token.kind() == SyntaxKind::STRING))
        {
            malformed(diagnostics);
        }
        return;
    }
    let Some(tree) = meta.token_tree() else {
        return;
    };
    let mut tokens = Vec::new();
    macros::tt::tokens_of(tree.syntax(), &mut tokens);
    let [open, inner @ .., close] = tokens.as_slice() else {
        malformed(diagnostics);
        return;
    };
    if open.kind != T!['('] || close.kind != T![')'] {
        malformed(diagnostics);
        return;
    }
    let mut seen = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() {
        let [key, eq, value, tail @ ..] = rest else {
            malformed(diagnostics);
            return;
        };
        if !matches!(key.text.as_str(), "since" | "note")
            || eq.kind != T![=]
            || value.kind != SyntaxKind::STRING
            || seen.contains(&key.text)
        {
            malformed(diagnostics);
            return;
        }
        seen.push(key.text);
        match tail {
            [] => break,
            [comma, tail @ ..] if comma.kind == T![,] => rest = tail,
            _ => {
                malformed(diagnostics);
                return;
            }
        }
    }
}

/// The hints of a `#[repr(...)]`, with their ranges; malformed ones are
/// reported as rustc does.
pub(super) fn repr_attr(
    attr: ast::Attr<'_>,
    out: &mut Vec<(Symbol, TextRange)>,
    repr_align: &mut u32,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let meta = attr.meta();
    let malformed = |diagnostics: &mut Vec<Diagnostic>| {
        let range = meta.map_or(attr.syntax().text_range(), |m| m.syntax().text_range());
        diagnostics.push(Diagnostic::new(range, "malformed `repr` attribute input"));
    };
    let Some(tree) = meta.and_then(|m| m.token_tree()) else {
        malformed(diagnostics);
        return;
    };
    let mut items: Vec<Vec<SyntaxElement<'_>>> = vec![Vec::new()];
    for element in tree.syntax().children_with_tokens() {
        match element.kind() {
            k if k.is_trivia() => {}
            SyntaxKind::L_PAREN | SyntaxKind::R_PAREN => {}
            SyntaxKind::COMMA => items.push(Vec::new()),
            _ => items.last_mut().expect("an item").push(element),
        }
    }
    let mut hints = Vec::new();
    for item in items.iter().filter(|i| !i.is_empty()) {
        let name = match item.as_slice() {
            [name] | [name, _] => name.into_token().filter(|t| t.kind() == SyntaxKind::IDENT),
            _ => None,
        };
        let known = name.is_some_and(|n| {
            let args = item.len() == 2;
            match n.text() {
                "align" => args && item[1].kind() == SyntaxKind::TOKEN_TREE,
                "C" | "Rust" | "transparent" | "packed" | "simd" => {
                    !args || (n.text() == "packed" && item[1].kind() == SyntaxKind::TOKEN_TREE)
                }
                other => !args && REPR_INTS.contains(&other),
            }
        });
        match name {
            Some(name) if known => {
                if name.text() == "align" {
                    let tree = item[1].into_node().expect("checked token tree");
                    let mut args: Vec<_> = tree
                        .children_with_tokens()
                        .filter(|e| {
                            !e.kind().is_trivia()
                                && !matches!(e.kind(), SyntaxKind::L_PAREN | SyntaxKind::R_PAREN)
                        })
                        .collect();
                    if args.last().is_some_and(|e| e.kind() == SyntaxKind::COMMA) {
                        args.pop();
                    }
                    let value = match args.as_slice() {
                        [arg] => arg
                            .into_token()
                            .filter(|t| t.kind() == SyntaxKind::INT_NUMBER)
                            .and_then(|t| crate::literal::parse_int(t.text()).ok()),
                        _ => None,
                    };
                    let range = tree.text_range();
                    match value {
                        Some(crate::Literal::Int { value, suffix: None })
                            if value.is_power_of_two() && value <= (1 << 29) => {
                                *repr_align = (*repr_align).max(value as u32);
                            }
                        Some(crate::Literal::Int { suffix: None, .. }) => diagnostics.push(
                            Diagnostic::new(range, "invalid `repr(align)` attribute: alignment must be a power of two no larger than 2^29")),
                        _ => diagnostics.push(Diagnostic::new(range, "expected an unsuffixed integer alignment")),
                    }
                }
                hints.push((Symbol::intern(name.text()), name.text_range()));
            }
            _ => {
                malformed(diagnostics);
                return;
            }
        }
    }
    out.extend(hints);
}

/// Whether `hints`, a struct's `repr`s, say `C`; the hints flux does
/// not support are reported.
pub(super) fn struct_repr(
    hints: &[(Symbol, TextRange)],
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let mut c = false;
    for &(hint, range) in hints {
        match hint.as_str() {
            "C" => c = true,
            "Rust" | "align" | "transparent" => {}
            name if REPR_INTS.contains(&name) => diagnostics.push(Diagnostic::new(
                range,
                "attribute should be applied to an enum",
            )),
            name => diagnostics.push(Diagnostic::new(
                range,
                format!("the `repr({name})` attribute is not supported yet"),
            )),
        }
    }
    c
}

/// The integer type `hints`, an enum's `repr`s, give its discriminant;
/// hints flux does not support, and conflicting ones, are reported.
/// `has_fields`: a variant has fields, which `repr(C, u8)` describes;
/// `empty`: the enum has no variants, which no `repr` describes.
pub(super) fn enum_repr(
    hints: &[(Symbol, TextRange)],
    has_fields: bool,
    empty: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Symbol> {
    let mut int = None;
    let mut c = false;
    for &(hint, range) in hints {
        match hint.as_str() {
            "C" => c = true,
            "Rust" | "align" => {}
            name if REPR_INTS.contains(&name) => {
                if int.is_some() {
                    diagnostics.push(Diagnostic::new(
                        hints[0].1,
                        "conflicting representation hints",
                    ));
                    return int;
                }
                int = Some(hint);
            }
            name => diagnostics.push(Diagnostic::new(
                range,
                format!("the `repr({name})` attribute is not supported yet"),
            )),
        }
    }
    if c && int.is_some() && !has_fields {
        diagnostics.push(Diagnostic::new(
            hints[0].1,
            "conflicting representation hints",
        ));
    }
    if let Some(&(_, range)) = hints.iter().find(|h| h.0.as_str() != "Rust")
        && empty
        && (c || int.is_some() || hints.iter().any(|h| h.0.as_str() == "align"))
    {
        diagnostics.push(Diagnostic::new(
            range,
            "unsupported representation for zero-variant enum",
        ));
    }
    int
}
