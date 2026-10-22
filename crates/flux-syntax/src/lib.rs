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
    Children, ChildrenWithTokens, NodeId, SyntaxElement, SyntaxNode, SyntaxToken, SyntaxTree,
    TextRange, TokenOrigin,
};

/// Rust edition of one source crate. The default standalone-file mode is 2024.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Edition {
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
    let lexed = LexedStr::with_edition(&text, edition);
    let input = parser::Input::from_lexed(&lexed);
    let output = parser::parse_expansion(&input, fragment);
    let (parts, mut errors) = build::build(lexed, output);
    let mut tree = parts.into_tree(text);
    tree.set_origins(origins, call_site);
    for error in &mut errors {
        error.range = tree.map_text_range(error.range);
    }
    Parse { tree, errors }
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
