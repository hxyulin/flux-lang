//! Documentation on enabled items, using the same conditional attributes as HIR.
use crate::{CrateId, ItemTree};
use flux_syntax::{
    SyntaxElement, SyntaxKind, SyntaxNode,
    ast::{self, AstNode},
};
use std::path::PathBuf;

/// One item's Markdown and its source location.
#[derive(Debug)]
pub struct Documentation {
    pub path: PathBuf,
    pub line: usize,
    pub markdown: String,
}
/// Reads documentation only from files belonging to this crate. Disabled items
/// and function-local items do not contribute examples.
pub fn documentation(items: &ItemTree, krate: CrateId) -> Result<Vec<Documentation>, String> {
    let files: std::collections::BTreeSet<_> = items
        .modules
        .iter()
        .filter(|(_, m)| m.krate == krate && m.block.is_none())
        .map(|(_, m)| m.file)
        .collect();
    let mut result = Vec::new();
    for file in files {
        let source = &items.files[&file];
        walk(source.parse.tree().root(), &source.path, &mut result)?;
    }
    Ok(result)
}
fn walk(
    node: SyntaxNode<'_>,
    path: &std::path::Path,
    result: &mut Vec<Documentation>,
) -> Result<(), String> {
    if !crate::item_tree::item_enabled(node, &mut Vec::new()) {
        return Ok(());
    }
    let mut markdown = String::new();
    for element in node.children_with_tokens() {
        if let SyntaxElement::Token(token) = element {
            let text = token.text();
            if (text.starts_with("///") && !text.starts_with("////")) || text.starts_with("//!") {
                markdown.push_str(&text[3..]);
                markdown.push('\n');
            } else if (text.starts_with("/**") && !text.starts_with("/***"))
                || text.starts_with("/*!")
            {
                for line in text[3..].strip_suffix("*/").unwrap_or(&text[3..]).lines() {
                    markdown.push_str(line.trim_start().strip_prefix("* ").unwrap_or(line));
                    markdown.push('\n');
                }
            }
        } else if let SyntaxElement::Node(child) = element
            && let Some(attr) = ast::Attr::cast(child)
        {
            attribute_markdown(attr, path, &mut markdown)?;
        }
    }
    if !markdown.is_empty() {
        result.push(Documentation {
            path: path.to_owned(),
            line: node.tree().text()
                [..(node.text_range().start() - node.tree().root().text_range().start()) as usize]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1,
            markdown,
        });
    }
    for child in node.children() {
        if child.kind() == SyntaxKind::ATTR
            || (node.kind() == SyntaxKind::FN && child.kind() == SyntaxKind::BLOCK_EXPR)
        {
            continue;
        }
        walk(child, path, result)?;
    }
    Ok(())
}

fn attribute_markdown(
    attr: ast::Attr<'_>,
    path: &std::path::Path,
    markdown: &mut String,
) -> Result<(), String> {
    let attrs = crate::cfg::Attrs::new(std::iter::once(attr));
    for attr in attrs
        .iter()
        .filter(|attr| crate::item_tree::attr_name(*attr) == Some("doc"))
    {
        if let Some(expr) = attr.meta().and_then(|m| m.expr()) {
            match expr {
                ast::Expr::Literal(literal) => {
                    if let Some(value) = literal.value() {
                        markdown.push_str(&crate::literal::unescape_str(value.text())?);
                        markdown.push('\n');
                    }
                }
                ast::Expr::MacroExpr(call)
                    if call
                        .path()
                        .and_then(|p| p.segment())
                        .and_then(|s| s.name_ref())
                        .is_some_and(|n| n.text() == "include_str") =>
                {
                    let tree = call.token_tree().ok_or("expected include_str path")?;
                    let mut tokens = Vec::new();
                    crate::macros::tt::tokens_of(tree.syntax(), &mut tokens);
                    let token = tokens
                        .iter()
                        .find(|token| token.kind == SyntaxKind::STRING)
                        .ok_or("expected a literal documentation path")?;
                    let relative = crate::literal::unescape_str(token.text.as_str())?;
                    let included = path
                        .parent()
                        .ok_or("documentation has no parent")?
                        .join(relative);
                    markdown.push_str(
                        &std::fs::read_to_string(&included)
                            .map_err(|e| format!("{}: {e}", included.display()))?,
                    );
                    markdown.push('\n');
                }
                _ => {
                    return Err(format!(
                        "{}: this macro-valued documentation is not supported yet",
                        path.display()
                    ));
                }
            }
        }
    }
    Ok(())
}
