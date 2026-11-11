//! Lexer, parser and lossless syntax trees for flux.
//!
//! Parsing happens in three stages, each a plain function over owned data:
//!
//! 1. [`LexedStr::new`] splits text into tokens, trivia included.
//! 2. The parser reads the significant token kinds and emits events.
//! 3. [`build_tree`] combines events and tokens into a [`SyntaxTree`].
//!
//! A [`SyntaxTree`] is a few flat arrays that own the text, so it is
//! `Send + Sync` and files can be parsed on separate threads. A
//! [`SyntaxNode`] is a `Copy` handle into it: a reference and an index.
//!
//! ```
//! let parse = flux_syntax::parse_source_file("fn main() {}");
//! assert!(parse.errors().is_empty());
//! assert_eq!(parse.root().to_string(), "fn main() {}");
//! ```

pub mod ast;
mod build;
pub mod lexer;
pub mod parser;
mod syntax_kind;
mod tree;

pub use build::build_tree;
pub use lexer::LexedStr;
pub use syntax_kind::SyntaxKind;
pub use tree::{
    CfgOptions, Children, ChildrenWithTokens, NodeId, PanicStrategy, SyntaxElement, SyntaxNode,
    SyntaxToken, SyntaxTree, TextRange, TokenOrigin,
};

/// Rust edition of one source crate. The default standalone-file mode is 2024.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Edition {
    E2015,
    E2018,
    E2021,
    #[default]
    E2024,
}

/// A problem found while lexing or parsing. Parsing always produces a
/// tree; errors describe where it differs from valid flux.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SyntaxError {
    pub message: String,
    pub range: TextRange,
}

/// The result of parsing: a tree and its errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    tree: SyntaxTree,
    errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }

    pub fn root(&self) -> SyntaxNode<'_> {
        self.tree.root()
    }

    /// Assigns a physical fragment a distinct coordinate interval. Raw ranges
    /// still index its text; mapped ranges can cross body/file boundaries.
    pub fn with_source_offset(mut self, base: u32) -> Self {
        let lexed = LexedStr::new(self.tree.text());
        let origins: Vec<_> = (0..lexed.len())
            .filter(|&i| !lexed.kind(i).is_trivia())
            .map(|i| {
                let range = lexed.token_range(i);
                (
                    range.start(),
                    TokenOrigin {
                        range: TextRange::new(base + range.start(), base + range.end()),
                        group: 1,
                        ctxt: 0,
                    },
                )
            })
            .collect();
        self.tree.set_origins(
            &origins,
            TextRange::new(base, base + self.tree.text().len() as u32),
        );
        for error in &mut self.errors {
            error.range = TextRange::new(base + error.range.start(), base + error.range.end());
        }
        self
    }

    /// Attaches compilation inputs without changing the source or its ranges.
    pub fn with_cfg_options(mut self, cfg: Option<std::sync::Arc<CfgOptions>>) -> Self {
        self.tree.set_cfg_options(cfg);
        self
    }

    /// The root as a typed node.
    pub fn source_file(&self) -> ast::SourceFile<'_> {
        ast::AstNode::cast(self.root()).expect("the root is a SOURCE_FILE")
    }

    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// The tree, one element per line, followed by the errors. Used by
    /// snapshot tests and `flux parse`.
    pub fn debug_dump(&self) -> String {
        let mut out = format!("{:#?}", self.root());
        for error in &self.errors {
            out.push_str(&format!("error {:?}: {}\n", error.range, error.message));
        }
        out
    }
}

/// Parses a whole source file, copying `text` into the tree.
pub fn parse_source_file(text: &str) -> Parse {
    parse_source_text(text.into())
}

/// Parses a whole source file, which the tree takes ownership of.
pub fn parse_source_text(text: Box<str>) -> Parse {
    parse_source_text_with_edition(text, Edition::E2024)
}

/// Parses owned source under the selected crate edition.
pub fn parse_source_text_with_edition(text: Box<str>, edition: Edition) -> Parse {
    let lexed = LexedStr::with_edition(&text, edition);
    let input = parser::Input::from_lexed(&lexed);
    let output = parser::parse_source_file(&input);
    let (parts, errors) = build::build(lexed, output);
    Parse {
        tree: parts.into_tree(text),
        errors,
    }
}

/// Parses an included physical file in the syntactic position of `include!`.
/// Ranges and hygiene belong to this file, rather than the macro invocation.
pub fn parse_included_source(
    text: Box<str>,
    fragment: parser::Fragment,
    edition: Edition,
) -> Parse {
    let lexed = LexedStr::with_edition(&text, edition);
    let input = parser::Input::from_lexed(&lexed);
    let output = parser::parse_expansion(&input, fragment);
    let (parts, errors) = build::build(lexed, output);
    let tree = parts.into_tree(text);
    Parse { tree, errors }
}

/// Parses the text of a macro expansion as `fragment` (see
/// [`parser::parse_expansion`]). `origins` gives, for each of the
/// expander's tokens, the offset in `text` it starts at and where it
/// came from, which become the tree's ranges, as do its errors'; nodes
/// without one place of their own are at `call_site`.
pub fn parse_expansion(
    text: Box<str>,
    origins: &[(u32, TokenOrigin)],
    call_site: TextRange,
    fragment: parser::Fragment,
) -> Parse {
    parse_expansion_with_edition(text, origins, call_site, fragment, Edition::E2024)
}

/// Parses an expansion under its source crate edition.
pub fn parse_expansion_with_edition(
    text: Box<str>,
    origins: &[(u32, TokenOrigin)],
    call_site: TextRange,
    fragment: parser::Fragment,
    edition: Edition,
) -> Parse {
    parse_expansion_with_token_kinds(text, origins, &[], call_site, fragment, edition)
}

/// Parses expanded tokens without changing identifiers from their source
/// edition when a macro crosses a crate edition boundary.
pub fn parse_expansion_with_token_kinds(
    text: Box<str>,
    origins: &[(u32, TokenOrigin)],
    kinds: &[(u32, SyntaxKind)],
    call_site: TextRange,
    fragment: parser::Fragment,
    edition: Edition,
) -> Parse {
    let mut lexed = LexedStr::with_edition(&text, edition);
    lexed.set_token_kinds(kinds);
    let mut input = parser::Input::with_edition(edition);
    for (index, &kind) in lexed.kinds().iter().enumerate() {
        if kind.is_trivia() {
            continue;
        }
        input.push_contextual(
            kind,
            lexed.is_union_keyword(index),
            lexed.is_dyn_keyword(index),
        );
        if lexed.is_default_keyword(index) {
            input.set_last_default_keyword();
        }
        if lexed.is_safe_keyword(index) {
            input.set_last_safe_keyword();
        }
        if kind == T!['(']
            && origins
                .binary_search_by_key(&lexed.token_start(index), |(start, _)| *start)
                .ok()
                .is_some_and(|at| origins[at].1.range.is_empty())
        {
            input.set_last_invisible_paren();
        }
        if index + 1 < lexed.len() && !lexed.kind(index + 1).is_trivia() {
            input.set_last_joint();
        }
    }
    let output = parser::parse_expansion(&input, fragment);
    let (parts, mut errors) = build::build(lexed, output);
    let mut tree = parts.into_tree(text);
    tree.set_origins(origins, call_site);
    for error in &mut errors {
        error.range = tree.map_text_range(error.range);
    }
    Parse { tree, errors }
}

impl Parse {
    /// Retains each macro token's source edition after parser compaction.
    pub fn with_token_editions(mut self, editions: &[(u32, Edition)]) -> Self {
        self.tree.set_token_editions(editions);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Parse>();
    }

    #[test]
    fn navigation() {
        let parse = parse_source_file("// c\nfn f(a: u8) {}\n");
        let root = parse.root();
        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);
        let f = root.first_child().unwrap();
        assert_eq!(f.kind(), SyntaxKind::FN);
        assert_eq!(f.text(), "// c\nfn f(a: u8) {}");
        assert_eq!(f.parent(), Some(root));
        assert_eq!(root.parent(), None);
        let params = f
            .children()
            .find(|n| n.kind() == SyntaxKind::PARAM_LIST)
            .unwrap();
        assert_eq!(params.text(), "(a: u8)");
        for node in root.descendants().skip(1) {
            let parent = node.parent().unwrap();
            assert!(parent.children().any(|c| c == node));
        }
        let kinds: Vec<_> = f.children_with_tokens().map(|e| e.kind()).collect();
        use SyntaxKind::*;
        assert_eq!(
            kinds,
            [
                COMMENT, WHITESPACE, FN_KW, WHITESPACE, NAME, PARAM_LIST, WHITESPACE, BLOCK_EXPR
            ]
        );
        parse.tree().validate().unwrap();
    }
}
