//! Attribute admission and representation hints.
use super::{
    AstChildren, AstNode, Diagnostic, NativeKind, NativeLibrary, REPR_INTS, Symbol, SyntaxElement,
    SyntaxKind, T, TextRange, ast, cfg_position, macros, unsupported,
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

/// A bare attribute has neither arguments nor an unsafe wrapper.
pub(super) fn bare_attr(attr: ast::Attr<'_>) -> bool {
    attr.meta().is_some_and(|m| {
        m.token_tree().is_none() && m.eq_token().is_none() && m.unsafe_token().is_none()
    })
}

/// rustc parses limit attributes as decimal host-sized integer strings.
pub(super) fn integer_limit_attr(attr: ast::Attr<'_>) -> Option<usize> {
    let token = attr
        .meta()
        .filter(|m| {
            m.unsafe_token().is_none() && m.token_tree().is_none() && m.eq_token().is_some()
        })?
        .expr()
        .and_then(|e| match e {
            ast::Expr::Literal(l) => l.value(),
            _ => None,
        })?;
    if token.kind() != SyntaxKind::STRING {
        return None;
    }
    let value = crate::literal::unescape_str(token.text()).ok()?;
    value.parse().ok()
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
        if let Some(name @ ("recursion_limit" | "type_length_limit")) = attr_name(attr) {
            if integer_limit_attr(attr).is_none() {
                diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    format!(
                        "{} must be a non-negative integer string",
                        name.replace('_', " ")
                    ),
                ));
            }
            // Stable rustc does not enforce type_length_limit. Like its unused
            // attribute warning, non-root occurrences of either have no effect.
            continue;
        }
        if attr_name(attr) == Some("macro_use") {
            if !bare_attr(attr) {
                diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    "arguments to `macro_use` are not allowed here",
                ));
            }
            continue;
        }
        if crate_root && matches!(attr_name(attr), Some("crate_name" | "crate_type")) {
            let name = attr_name(attr).unwrap();
            let value = attr
                .meta()
                .filter(|m| {
                    m.unsafe_token().is_none() && m.token_tree().is_none() && m.eq_token().is_some()
                })
                .and_then(|m| m.expr())
                .and_then(|e| match e {
                    ast::Expr::Literal(l) => l.value(),
                    _ => None,
                })
                .filter(|t| t.kind() == SyntaxKind::STRING)
                .and_then(|t| crate::literal::unescape_str(t.text()).ok());
            match value {
                Some(value)
                    if name == "crate_name"
                        && !value.is_empty()
                        && value
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_') => {}
                Some(value)
                    if name == "crate_type" && matches!(value.as_str(), "lib" | "rlib" | "bin") => {
                }
                Some(_) if name == "crate_type" => unsupported(
                    diagnostics,
                    attr.syntax().text_range(),
                    "this crate output kind",
                ),
                _ => diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    format!("malformed `{name}` attribute input"),
                )),
            }
            continue;
        }
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
        // Rust ignores feature gates placed on a module rather than a crate.
        if !crate_root
            && attr_name(attr) == Some("feature")
            && attr
                .meta()
                .is_some_and(|m| m.token_tree().is_some() && m.unsafe_token().is_none())
        {
            continue;
        }
        if !(is_core && attr_name(attr).is_some_and(|n| ["no_core", "feature"].contains(&n))) {
            check_attr(attr, diagnostics);
        }
    }
}

/// Only baseline NEON and scalar CRC implementations on explicitly unsafe
/// functions are supported. Neither requires extra codegen ISA flags, and
/// ordinary unsafe function items retain all call/coercion rules.
pub(super) fn target_feature_attr(
    attr: ast::Attr<'_>,
    function: ast::Fn<'_>,
    in_extern_block: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Symbol> {
    let mut enabled = Vec::new();
    let range = attr.syntax().text_range();
    let meta = attr.meta().expect("named attribute");
    if !matches!(std::env::consts::ARCH, "aarch64" | "x86_64") {
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
        return enabled;
    }
    if in_extern_block || function.body().is_none() {
        diagnostics.push(Diagnostic::new(
            range,
            "`target_feature` requires a function with a body",
        ));
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
        return enabled;
    };
    if meta.eq_token().is_some() || open.kind != T!['('] || close.kind != T![')'] {
        malformed(diagnostics);
        return enabled;
    }
    let mut rest = inner;
    // Empty lists are inert in rustc. Repeated keys, attributes and features
    // are accepted too; validate each rather than silently dropping duplicates.
    while !rest.is_empty() {
        let [key, eq, value, tail @ ..] = rest else {
            malformed(diagnostics);
            return enabled;
        };
        if key.text.as_str() != "enable" || eq.kind != T![=] || value.kind != SyntaxKind::STRING {
            malformed(diagnostics);
            return enabled;
        }
        match crate::literal::unescape_str(value.text.as_str()) {
            Ok(features) => {
                for feature in features.split(',') {
                    if !match std::env::consts::ARCH {
                        "aarch64" => {
                            matches!(feature, "neon" | "crc" | "sha2" | "sha3" | "dit" | "sb")
                        }
                        "x86_64" => matches!(
                            feature,
                            "sse"
                                | "sse2"
                                | "sse3"
                                | "ssse3"
                                | "sse4.1"
                                | "sse4.2"
                                | "avx"
                                | "avx2"
                                | "avx512f"
                                | "pclmulqdq"
                                | "vpclmulqdq"
                                | "sha"
                                | "xsave"
                        ),
                        _ => false,
                    } {
                        diagnostics.push(Diagnostic::new(
                            value.range(),
                            format!("target feature `{feature}` is not supported yet"),
                        ));
                    } else {
                        let feature = Symbol::intern(feature);
                        if !enabled.contains(&feature) {
                            enabled.push(feature);
                        }
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
                return enabled;
            }
        }
    }
    enabled
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

/// The hint of a well-formed `#[inline]`, `#[inline(always)]` or
/// `#[inline(never)]`.
pub(super) fn inline_attr(attr: ast::Attr<'_>) -> Option<super::InlineHint> {
    let meta = attr.meta()?;
    if meta.unsafe_token().is_some() || meta.eq_token().is_some() {
        return None;
    }
    let Some(tree) = meta.token_tree() else {
        return Some(super::InlineHint::Hint);
    };
    let mut tokens = Vec::new();
    macros::tt::tokens_of(tree.syntax(), &mut tokens);
    match tokens.as_slice() {
        [open, arg, close] if open.kind == T!['('] && close.kind == T![')'] => {
            match arg.text.as_str() {
                "always" => Some(super::InlineHint::Always),
                "never" => Some(super::InlineHint::Never),
                _ => None,
            }
        }
        _ => None,
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

/// Whether `path` names an attribute of a tool rustc registers by default
/// (`#[clippy::has_significant_drop]`, `#[rustfmt::skip]`). rustc leaves
/// these to the tool and accepts any input after the path.
fn tool_attr_path(path: ast::Path<'_>) -> bool {
    let Some(mut root) = path.qualifier() else {
        return false;
    };
    while let Some(qualifier) = root.qualifier() {
        root = qualifier;
    }
    root.segment().is_some_and(|s| {
        s.colon2_token().is_none()
            && s.generic_arg_list().is_none()
            && s.name_ref().is_some_and(|n| {
                matches!(n.text(), "clippy" | "rustfmt" | "miri" | "rust_analyzer")
            })
    })
}

pub(crate) fn check_attr(attr: ast::Attr<'_>, diagnostics: &mut Vec<Diagnostic>) {
    const IGNORED: &[&str] = &[
        "allow", "warn", "deny", "forbid", "expect", "inline", "cold", "must_use", "doc",
    ];
    let name = attr_name(attr);
    // Rust treats `macro_use` on an import as an unused attribute.
    if name == Some("macro_use")
        && bare_attr(attr)
        && attr
            .syntax()
            .parent()
            .is_some_and(|p| p.kind() == SyntaxKind::USE)
    {
        return;
    }
    // This compiler hint changes diagnostic suggestions, not trait selection.
    if let Some(meta) = attr.meta()
        && meta.unsafe_token().is_none()
        && let Some(path) = meta.path()
        && path
            .segment()
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "do_not_recommend")
        && let Some(namespace) = path.qualifier()
        && namespace.qualifier().is_none()
        && namespace
            .segment()
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "diagnostic")
    {
        return;
    }
    // This is lint metadata, including when rustc warns about its placement.
    // It never substitutes for checking the annotated item or impl.
    if name == Some("automatically_derived") {
        let meta = attr.meta().expect("named attribute");
        if meta.unsafe_token().is_some() {
            diagnostics.push(Diagnostic::new(
                attr.syntax().text_range(),
                "`automatically_derived` is not an unsafe attribute",
            ));
        } else if meta.token_tree().is_some() || meta.eq_token().is_some() {
            diagnostics.push(Diagnostic::new(
                attr.syntax().text_range(),
                "malformed `automatically_derived` attribute input",
            ));
        }
        return;
    }
    if name == Some("link_name") {
        link_name_attr(attr, diagnostics);
        return;
    }
    if name == Some("link") {
        link_attr(attr, diagnostics);
        return;
    }
    if name == Some("deprecated") {
        deprecated_attr(attr, diagnostics);
        return;
    }
    if let Some(meta) = attr.meta()
        && meta.unsafe_token().is_none()
        && meta.path().is_some_and(tool_attr_path)
    {
        if attr.bang_token().is_some() {
            diagnostics.push(Diagnostic::new(
                attr.syntax().text_range(),
                "custom inner attributes are unstable",
            ));
        }
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

/// Native names are safe string-valued attributes; unused placements have
/// no effect, as rustc's unused-attribute diagnostic does not reject them.
pub(super) fn link_name_attr(
    attr: ast::Attr<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Symbol> {
    let range = attr.syntax().text_range();
    let meta = attr.meta()?;
    if meta.unsafe_token().is_some() {
        diagnostics.push(Diagnostic::new(
            range,
            "`link_name` is not an unsafe attribute",
        ));
        return None;
    }
    let literal = if meta.eq_token().is_some() && meta.token_tree().is_none() {
        match meta.expr() {
            Some(ast::Expr::Literal(literal)) => {
                literal.value().filter(|t| t.kind() == SyntaxKind::STRING)
            }
            _ => None,
        }
    } else {
        None
    };
    let Some(literal) = literal else {
        let message = if matches!(meta.expr(), Some(ast::Expr::MacroExpr(_))) {
            "macro-valued link names are not supported yet"
        } else {
            "malformed `link_name` attribute input"
        };
        diagnostics.push(Diagnostic::new(range, message));
        return None;
    };
    let name = match crate::literal::unescape_str(literal.text()) {
        Ok(name) => name,
        Err(message) => {
            diagnostics.push(Diagnostic::new(range, message));
            return None;
        }
    };
    if name.is_empty() || name.contains('\0') {
        diagnostics.push(Diagnostic::new(
            range,
            "link name must be nonempty and contain no NUL characters",
        ));
        return None;
    }
    if name.starts_with('\u{1}') {
        diagnostics.push(Diagnostic::new(
            range,
            "raw assembler-prefixed link names are not supported yet",
        ));
        return None;
    }
    Some(Symbol::intern(&name))
}

/// Literal native-library requests of the kinds the linker distinguishes:
/// `dylib` (the default), `static` and `framework`.
pub(super) fn link_attr(
    attr: ast::Attr<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<NativeLibrary> {
    let range = attr.syntax().text_range();
    let meta = attr.meta()?;
    let Some(tree) = meta
        .token_tree()
        .filter(|_| meta.unsafe_token().is_none() && meta.eq_token().is_none())
    else {
        diagnostics.push(Diagnostic::new(range, "malformed `link` attribute input"));
        return None;
    };
    let tokens: Vec<_> = tree
        .syntax()
        .children_with_tokens()
        .filter(|t| !t.kind().is_trivia())
        .collect();
    let mut tokens = &tokens[1..tokens.len().saturating_sub(1)];
    let mut name = None;
    let mut kind = None;
    while !tokens.is_empty() {
        let [
            SyntaxElement::Token(key),
            SyntaxElement::Token(eq),
            SyntaxElement::Token(value),
            rest @ ..,
        ] = tokens
        else {
            diagnostics.push(Diagnostic::new(
                range,
                "expected literal native library options",
            ));
            return None;
        };
        if eq.text() != "=" || value.kind() != SyntaxKind::STRING {
            diagnostics.push(Diagnostic::new(
                range,
                "expected literal native library options",
            ));
            return None;
        }
        let value = match crate::literal::unescape_str(value.text()) {
            Ok(value) => value,
            Err(message) => {
                diagnostics.push(Diagnostic::new(range, message));
                return None;
            }
        };
        let slot = match key.text() {
            "name" => &mut name,
            "kind" => &mut kind,
            _ => {
                unsupported(
                    diagnostics,
                    range,
                    "native library options other than name and kind",
                );
                return None;
            }
        };
        if slot.replace(value).is_some() {
            diagnostics.push(Diagnostic::new(range, "duplicate native library option"));
            return None;
        }
        tokens = match rest {
            [] => rest,
            [SyntaxElement::Token(comma), rest @ ..] if comma.text() == "," => rest,
            _ => {
                diagnostics.push(Diagnostic::new(range, "expected `,`"));
                return None;
            }
        };
    }
    let kind = match kind.as_deref() {
        None | Some("dylib") => NativeKind::Dylib,
        Some("static") => NativeKind::Static,
        Some("framework") => NativeKind::Framework,
        Some(_) => {
            unsupported(
                diagnostics,
                range,
                "native library kinds other than dylib, static and framework",
            );
            return None;
        }
    };
    let Some(name) = name.filter(|name| !name.is_empty() && !name.contains('\0')) else {
        diagnostics.push(Diagnostic::new(
            range,
            "native library name must be nonempty and contain no NUL characters",
        ));
        return None;
    };
    if name.contains('/') || name.contains('\\') {
        unsupported(diagnostics, range, "native library path names");
        return None;
    }
    Some(NativeLibrary {
        name: Symbol::intern(&name),
        kind,
    })
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
    repr_pack: &mut Option<u32>,
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
                if name.text() == "packed" && item.len() == 1 {
                    if repr_pack.is_some_and(|pack| pack != 1) {
                        diagnostics.push(Diagnostic::new(
                            name.text_range(),
                            "conflicting packed representation hints",
                        ));
                    }
                    *repr_pack = Some(1);
                }
                if name.text() == "align" || (name.text() == "packed" && item.len() == 2) {
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
                                if name.text()=="align" {*repr_align=(*repr_align).max(value as u32);} else {
                                    if repr_pack.is_some_and(|pack|pack!=value as u32){diagnostics.push(Diagnostic::new(range,"conflicting packed representation hints"));}
                                    *repr_pack=Some(value as u32);
                                }
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
            "Rust" | "align" | "transparent" | "packed" => {}
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
