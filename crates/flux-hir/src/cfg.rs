//! Conditional compilation shared by item attributes and `cfg!`.

use crate::macros::tt::Tok;
use flux_syntax::{SyntaxKind, T, TextRange};

type Error = (TextRange, String);

/// The value of `cfg!(predicate)`, for flux's target.
pub(crate) fn eval(
    call: TextRange,
    toks: &[Tok],
    cfg: Option<&flux_syntax::CfgOptions>,
) -> Result<bool, Error> {
    let mut pos = 0;
    let value = cfg_predicate(call, toks, &mut pos, cfg)?;
    if toks.get(pos).is_some_and(|t| t.kind == T![,]) {
        pos += 1;
    }
    if let Some(t) = toks.get(pos) {
        return Err((t.range(), "expected one cfg-pattern".into()));
    }
    Ok(value)
}

fn cfg_predicate(
    call: TextRange,
    toks: &[Tok],
    pos: &mut usize,
    cfg: Option<&flux_syntax::CfgOptions>,
) -> Result<bool, Error> {
    let Some(name) = toks
        .get(*pos)
        .filter(|t| t.kind == SyntaxKind::IDENT || t.kind.is_keyword())
    else {
        let at = toks.get(*pos).map_or(call, Tok::range);
        return Err((at, "expected identifier".into()));
    };
    *pos += 1;
    // `cfg(true)` and `cfg(false)` are literals, not names.
    match name.kind {
        T![true] => return Ok(true),
        T![false] => return Ok(false),
        _ => {}
    }
    let name = name.text.as_str();
    if matches!(name, "all" | "any" | "not") && toks.get(*pos).is_some_and(|t| t.kind == T!['(']) {
        *pos += 1;
        let mut values = Vec::new();
        while toks.get(*pos).is_some_and(|t| t.kind != T![')']) {
            values.push(cfg_predicate(call, toks, pos, cfg)?);
            if toks.get(*pos).is_some_and(|t| t.kind == T![,]) {
                *pos += 1;
            } else if toks.get(*pos).is_some_and(|t| t.kind != T![')']) {
                return Err((toks[*pos].range(), "expected `,`".into()));
            }
        }
        if !toks.get(*pos).is_some_and(|t| t.kind == T![')']) {
            return Err((call, "expected `)`".into()));
        }
        *pos += 1;
        return match name {
            "all" => Ok(values.iter().all(|&v| v)),
            "any" => Ok(values.iter().any(|&v| v)),
            _ => match values.as_slice() {
                [v] => Ok(!v),
                _ => Err((call, "expected 1 cfg-pattern".into())),
            },
        };
    }
    let mut value = None;
    if toks.get(*pos).is_some_and(|t| t.kind == T![=]) {
        *pos += 1;
        match toks.get(*pos) {
            Some(t) if t.kind == SyntaxKind::STRING => {
                value = Some(
                    crate::literal::unescape_str(t.text.as_str())
                        .map_err(|message| (t.range(), message))?,
                );
                *pos += 1;
            }
            Some(t) => return Err((t.range(), "expected a string literal".into())),
            None => return Err((call, "expected a string literal".into())),
        }
    }
    let os = std::env::consts::OS;
    Ok(match (name, value.as_deref()) {
        // On unless the crate was configured without them, as rustc's
        // `-C debug-assertions`; the embedded libraries keep them on.
        ("debug_assertions", None) => cfg.is_none_or(|cfg| cfg.debug_assertions),
        ("panic", Some(v)) => v == "abort",
        ("unix", None) => cfg!(unix),
        ("windows", None) => cfg!(windows),
        ("target_os", Some(v)) => v == os,
        ("target_family", Some(v)) => v == std::env::consts::FAMILY,
        ("target_arch", Some(v)) => v == std::env::consts::ARCH,
        ("target_pointer_width", Some(v)) => v == "64",
        ("target_endian", Some(v)) => v == "little",
        ("target_vendor", Some(v)) => v == if os == "macos" { "apple" } else { "unknown" },
        ("target_env", Some(v)) => v == if os == "macos" { "" } else { "gnu" },
        ("target_abi", Some(v)) => v.is_empty(),
        // Native byte/halfword/word/doubleword atomics are representable on
        // both admitted targets. The backend does not expose 128-bit atomics.
        ("target_has_atomic", Some(v)) => matches!(v, "8" | "16" | "32" | "64" | "ptr"),
        // Flux emits a fixed baseline target, independently of the flags used
        // to compile Flux itself. Only baseline features are advertised here;
        // optional CPU extensions require an explicit codegen configuration.
        ("target_feature", Some(v)) => match std::env::consts::ARCH {
            "aarch64" => v == "neon",
            "x86_64" => matches!(v, "fxsr" | "sse" | "sse2"),
            _ => false,
        },
        _ => cfg.is_some_and(|cfg| match value.as_deref() {
            None => cfg.names.iter().any(|candidate| &**candidate == name),
            Some(value) => cfg
                .values
                .iter()
                .any(|(key, candidate)| &**key == name && &**candidate == value),
        }),
    })
}

/// Expanded conditional attributes. Keeping the small expansion trees here lets
/// every attribute consumer see ordinary attributes, including derives and reprs.
pub(crate) struct Attrs<'a> {
    pub(crate) cfg: Option<std::sync::Arc<flux_syntax::CfgOptions>>,
    attrs: Vec<Attr<'a>>,
    pub(crate) errors: Vec<Error>,
}

enum Attr<'a> {
    Written(flux_syntax::ast::Attr<'a>),
    Expanded(flux_syntax::Parse),
}

impl<'a> Attrs<'a> {
    pub(crate) fn new(attrs: impl IntoIterator<Item = flux_syntax::ast::Attr<'a>>) -> Self {
        use flux_syntax::ast::AstNode;
        let mut attrs = attrs.into_iter().peekable();
        let cfg = attrs
            .peek()
            .and_then(|attr| attr.syntax().tree().cfg_options())
            .cloned();
        let mut result = Self {
            cfg,
            attrs: Vec::new(),
            errors: Vec::new(),
        };
        for attr in attrs {
            let mut toks = Vec::new();
            if crate::item_tree::attr_name(attr) == Some("cfg_attr")
                && attr.meta().is_some_and(|m| m.unsafe_token().is_none())
            {
                crate::macros::tt::tokens_of(attr.meta().unwrap().syntax(), &mut toks);
                result.expand(attr, &toks, 0);
            } else {
                result.attrs.push(Attr::Written(attr));
            }
        }
        result
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = flux_syntax::ast::Attr<'_>> {
        use flux_syntax::ast::AstNode;
        self.attrs.iter().map(|attr| match attr {
            Attr::Written(attr) => *attr,
            Attr::Expanded(parse) => parse
                .root()
                .descendants()
                .find_map(flux_syntax::ast::Attr::cast)
                .unwrap(),
        })
    }

    fn expand(&mut self, original: flux_syntax::ast::Attr<'a>, toks: &[Tok], depth: usize) {
        use flux_syntax::ast::AstNode;
        let range = original.syntax().text_range();
        if depth >= 128 {
            self.errors.push((
                range,
                "recursion limit reached while expanding `cfg_attr`".into(),
            ));
            return;
        }
        let result = (|| {
            let [_, open, inner @ .., close] = toks else {
                return Err((
                    range,
                    "expected `cfg_attr` predicate and attributes in parentheses".into(),
                ));
            };
            if open.kind != T!['('] || close.kind != T![')'] {
                return Err((
                    range,
                    "expected `cfg_attr` predicate and attributes in parentheses".into(),
                ));
            }
            let mut pos = 0;
            let enabled = cfg_predicate(range, inner, &mut pos, self.cfg.as_deref())?;
            if !inner.get(pos).is_some_and(|t| t.kind == T![,]) {
                return Err((range, "expected `,`".into()));
            }
            pos += 1;
            let rest = &inner[pos..];
            let mut parts = Vec::new();
            let mut start = 0;
            let mut nesting = 0;
            for (i, t) in rest.iter().enumerate() {
                match t.kind {
                    T!['('] | T!['['] | T!['{'] => nesting += 1,
                    T![')'] | T![']'] | T!['}'] => nesting -= 1,
                    T![,] if nesting == 0 => {
                        parts.push(&rest[start..i]);
                        start = i + 1;
                    }
                    _ => {}
                }
            }
            if start < rest.len() {
                parts.push(&rest[start..]);
            }
            if parts.iter().any(|p| p.is_empty()) {
                return Err((range, "expected an attribute".into()));
            }
            for part in parts {
                let mut text = String::from("#[");
                let mut origins = Vec::new();
                for t in part {
                    origins.push((text.len() as u32, t.origin));
                    text.push_str(t.text.as_str());
                    if !t.joint {
                        text.push(' ');
                    }
                }
                text.push_str("] struct __Cfg;");
                let parse = flux_syntax::parse_expansion(
                    text.into(),
                    &origins,
                    range,
                    flux_syntax::parser::Fragment::Items,
                )
                .with_cfg_options(self.cfg.clone());
                if let Some(error) = parse.errors().first() {
                    return Err((error.range, error.message.clone()));
                }
                if enabled {
                    if part.first().is_some_and(|t| t.text.as_str() == "cfg_attr") {
                        self.expand(original, part, depth + 1);
                    } else {
                        self.attrs.push(Attr::Expanded(parse));
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.errors.push(error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::tt::tokens_of;

    fn evaluate(text: &str) -> Result<bool, Error> {
        // Parsing retains every token even for an incomplete predicate.
        // Exercise the evaluator independently of delimiter recovery.
        let parse = flux_syntax::parse_source_file(text);
        let mut tokens = Vec::new();
        tokens_of(parse.root(), &mut tokens);
        eval(parse.root().text_range(), &tokens, None)
    }

    #[test]
    fn combinators_and_default_build() {
        for (text, expected) in [
            ("all()", true),
            ("any()", false),
            ("not(test)", true),
            ("all(not(test), any(test, all()),)", true),
            ("test", false),
            ("unknown", false),
            ("feature = \"enabled\"", false),
            ("target_os = \"imaginary\",", false),
            ("target_pointer_width = r#\"64\"#", true),
            ("true", true),
            ("false", false),
            ("r#true", false),
            ("all(true, not(false), any(false, true))", true),
            ("any(false, not(true))", false),
        ] {
            assert_eq!(evaluate(text), Ok(expected), "{text}");
        }
    }

    #[test]
    fn malformed_predicates_are_errors_even_in_false_branches() {
        for text in [
            "",
            "all(",
            "all(test",
            "all(test test)",
            "any(test not())",
            "not()",
            "not(test, unix)",
            "all(test, not())",
            "any(all(), not())",
            "test, unix",
            "target_os = 1",
            "unknown(test)",
            "all(test))",
            "all(test,,)",
            "target_os = \"\\q\"",
            "true = \"x\"",
            "all(false(test))",
        ] {
            assert!(evaluate(text).is_err(), "accepted {text}");
        }
    }
}
