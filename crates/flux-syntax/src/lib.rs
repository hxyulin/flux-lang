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
    TextRange,
};

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

/// Parses a whole source file.
pub fn parse_source_file(text: &str) -> Parse {
    let lexed = LexedStr::new(text);
    let input = parser::Input::from_lexed(&lexed);
    let output = parser::parse_source_file(&input);
    let (tree, errors) = build::build_tree(lexed, output);
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
