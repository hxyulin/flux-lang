//! Host procedural macros supplied explicitly by a source-graph driver.
use crate::SourceFile;
use crate::item_tree::{attr_name, item_enabled};
use flux_syntax::ast::{self, AstNode};
use flux_syntax::{Edition, SyntaxKind, TextRange, TokenOrigin};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The three Rust procedural-macro entry kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcMacroKind {
    Derive,
    Attribute,
    Function,
}

/// A token crossing the host boundary, retaining punctuation and source origin.
#[derive(Debug, Clone)]
pub struct ProcMacroToken {
    pub edition: Edition,
    pub kind: SyntaxKind,
    pub text: String,
    pub joint: bool,
    pub origin: TokenOrigin,
}

/// One expansion. The environment and source belong to the invocation crate.
#[derive(Debug, Clone)]
pub struct ProcMacroInput {
    /// The invocation crate index, scoped to this source graph.
    pub invocation_crate: u32,
    pub tokens: Vec<ProcMacroToken>,
    pub attributes: Option<Vec<ProcMacroToken>>,
    pub source: Arc<SourceFile>,
    pub call: TextRange,
    pub range_base: u32,
    pub mixed_context: u32,
    pub environment: BTreeMap<String, String>,
    pub current_dir: PathBuf,
}

/// Drivers implement host execution; HIR neither loads dylibs nor mutates env.
pub trait ProcMacroExpander: std::fmt::Debug + Send + Sync {
    fn expand(&self, input: ProcMacroInput) -> Result<Vec<ProcMacroToken>, String>;
}

/// A host export, optionally supplemented by checked helper declarations.
#[derive(Debug, Clone)]
pub struct ProcMacro {
    pub name: flux_intern::Symbol,
    pub kind: ProcMacroKind,
    /// `None` means helper metadata is unavailable, not that arbitrary
    /// attributes are allowed. Derives with unavailable metadata reject
    /// ambiguous bare attributes before attribute expansion.
    pub helpers: Option<Vec<flux_intern::Symbol>>,
    pub expander: Arc<dyn ProcMacroExpander>,
}

/// Reads literal host export declarations under their actual Cargo cfg inputs.
/// The host catalog supplies export names and kinds independently. Generated
/// exports absent from this map have no recovered helper metadata.
pub fn proc_macro_declarations(
    source: &SourceFile,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let mut result = std::collections::BTreeMap::new();
    for function in source.parse.root().children().filter_map(ast::Fn::cast) {
        // Host profile cfg differs from Flux's panic-abort target cfg. The
        // server lists exports but not their helpers, so never guess which
        // conditional helper declaration produced the host artifact.
        let exported = function.attrs().any(|attr| {
            attr.syntax().tokens().any(|token| {
                matches!(
                    token.text(),
                    "proc_macro" | "proc_macro_attribute" | "proc_macro_derive"
                )
            })
        });
        if exported
            && function.attrs().any(|attr| {
                matches!(attr_name(attr), Some("cfg" | "cfg_attr"))
                    && attr
                        .syntax()
                        .tokens()
                        .any(|token| matches!(token.text(), "panic" | "debug_assertions"))
            })
        {
            return Err(
                "host proc-macro declarations using profile cfg are not supported yet".into(),
            );
        }
        let mut diagnostics = Vec::new();
        if !item_enabled(function.syntax(), &mut diagnostics) {
            continue;
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics[0].message.clone());
        }
        let Some(name) = function.name() else {
            continue;
        };
        let attrs = crate::cfg::Attrs::new(function.attrs());
        if let Some((_, message)) = attrs.errors.first() {
            return Err(message.clone());
        }
        for attr in attrs.iter() {
            match attr_name(attr) {
                Some("proc_macro" | "proc_macro_attribute") => {
                    result.insert(name.text().to_owned(), Vec::new());
                }
                Some("proc_macro_derive") => {
                    let tree = attr
                        .meta()
                        .and_then(|m| m.token_tree())
                        .ok_or("missing proc-macro declaration arguments")?;
                    let identifiers = tree
                        .syntax()
                        .tokens()
                        .filter(|t| t.kind() == SyntaxKind::IDENT)
                        .map(|t| t.text().to_owned())
                        .collect::<Vec<_>>();
                    if let Some(name) = identifiers.first() {
                        if identifiers.get(1).is_some_and(|s| s != "attributes") {
                            return Err("unsupported proc-macro helper declaration".into());
                        }
                        result.insert(name.clone(), identifiers.into_iter().skip(2).collect());
                    }
                }
                _ => {}
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn host_profile_cfg_does_not_guess_helper_admission() {
        let mut db = crate::SourceDb::new();
        let root = db.add(
            "host.rs",
            r#"
            #[cfg(panic="abort")] #[proc_macro_derive(Shared, attributes(wrong))]
            pub fn first() {}
            #[cfg(panic="unwind")] #[proc_macro_derive(Shared, attributes(active))]
            pub fn second() {}
        "#,
        );
        assert!(
            proc_macro_declarations(db.file(root))
                .unwrap_err()
                .contains("profile cfg")
        );
    }
    #[test]
    fn helpers_follow_active_export_declarations() {
        let mut db = crate::SourceDb::new();
        let root = db.add(
            "host.rs",
            r#"
            #[cfg(any())] #[proc_macro_derive(Shared, attributes(inactive))]
            pub fn disabled() {}
            #[cfg_attr(all(), proc_macro_derive(Shared, attributes(active)))]
            pub fn enabled() {}
            #[proc_macro] pub fn r#identity() {}
        "#,
        );
        let declarations = proc_macro_declarations(db.file(root)).unwrap();
        assert_eq!(declarations.get("Shared"), Some(&vec!["active".into()]));
        assert_eq!(declarations.get("identity"), Some(&Vec::new()));
    }
}
