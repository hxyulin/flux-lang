//! Host procedural macros supplied explicitly by a source-graph driver.
use crate::item_tree::{attr_name, item_enabled};
use crate::{FileId, ItemTree, SourceDb, SourceFile};
use flux_syntax::ast::{self, AstNode};
use flux_syntax::{Edition, PanicStrategy, SyntaxKind, TextRange, TokenOrigin};
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Returns a private, immutable host identity that a graph-scoped resolver can restore.
    /// Custom expanders without a persistent host return `None`.
    fn flib_descriptor(&self) -> Option<Vec<u8>> {
        None
    }
}

/// Restores validated host execution without putting host loading in HIR.
/// Drivers create one resolver per compilation graph to share host state across exports.
pub trait ProcMacroResolver: std::fmt::Debug + Send + Sync {
    fn resolve(&self, descriptor: &[u8]) -> Result<Arc<dyn ProcMacroExpander>, String>;
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

/// Reads source declarations for the exports listed by the compiled host.
/// Fall back to item expansion only for derives the literal scan missed.
pub fn proc_macro_declarations(
    db: &mut SourceDb,
    root: FileId,
    derive_exports: &[&str],
) -> Result<BTreeMap<String, Vec<String>>, String> {
    let source = db.shared(root);
    let mut result = BTreeMap::new();
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
        for (name, helpers) in function_declarations(function)? {
            result.insert(name, helpers);
        }
    }
    if derive_exports.iter().all(|&name| result.contains_key(name)) {
        return Ok(result);
    }

    // The host protocol does not carry the build profile. Recover only
    // generated declarations whose helpers agree across those cfg inputs.
    let cfg = source.parse.tree().cfg_options().map(|cfg| (**cfg).clone());
    let mut common = None::<BTreeMap<String, Vec<String>>>;
    for debug_assertions in [true, false] {
        for panic_strategy in [PanicStrategy::Abort, PanicStrategy::Unwind] {
            let mut cfg = cfg.clone().unwrap_or_default();
            cfg.debug_assertions = debug_assertions;
            cfg.panic_strategy = panic_strategy;
            let configured = db.configured_file(root, Some(Arc::new(cfg)));
            let found = expanded_declarations(db, configured);
            if let Some(common) = &mut common {
                common.retain(|name, helpers| found.get(name) == Some(helpers));
                if common.is_empty() {
                    return Ok(result);
                }
            } else {
                if found.is_empty() {
                    return Ok(result);
                }
                common = Some(found);
            }
        }
    }
    for (name, helpers) in common.unwrap_or_default() {
        if derive_exports.contains(&name.as_str()) {
            result.entry(name).or_insert(helpers);
        }
    }
    Ok(result)
}

fn expanded_declarations(db: &mut SourceDb, root: FileId) -> BTreeMap<String, Vec<String>> {
    let items = ItemTree::new(
        db,
        root,
        flux_intern::Symbol::intern("__host_macros"),
        false,
    );
    let module = items.module_of(items.main_crate());
    let Some(expansions) = root_item_expansions(db, &items, root) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    let mut ambiguous = std::collections::BTreeSet::new();
    for (_, function) in items.functions.iter() {
        if function.loc.module != module
            || function.owner.is_some()
            || function.loc.file == root
            || !expansions.contains(&function.loc.file)
        {
            continue;
        }
        let tree = db.file(function.loc.file).parse.tree();
        if let Some(function) = ast::Fn::cast(tree.node(function.node)) {
            let Ok(declarations) = function_declarations(function) else {
                return BTreeMap::new();
            };
            for (name, helpers) in declarations {
                if result.insert(name.clone(), helpers).is_some() {
                    ambiguous.insert(name);
                }
            }
        }
    }
    result.retain(|name, _| !ambiguous.contains(name));
    result
}

/// An unresolved item expansion can introduce a textual macro that shadows a
/// later call. Use generated helpers only from a crate root we can expand.
fn root_item_expansions(
    db: &SourceDb,
    items: &ItemTree,
    root: FileId,
) -> Option<rustc_hash::FxHashSet<FileId>> {
    let by_call: rustc_hash::FxHashMap<_, _> = items
        .expansion_sites
        .iter()
        .map(|(&file, &call)| (call, file))
        .collect();
    let mut files = rustc_hash::FxHashSet::default();
    let mut pending = vec![root];
    while let Some(file) = pending.pop() {
        files.insert(file);
        let root = db.file(file).parse.root();
        if !inert_host_attributes(root) || uses_rustc_target_cfg(root) {
            return None;
        }
        for item in root.children().filter_map(ast::Item::cast) {
            let item = item.syntax();
            if uses_rustc_target_cfg(item) {
                return None;
            }
            let mut errors = Vec::new();
            let enabled = item_enabled(item, &mut errors);
            if !errors.is_empty() {
                return None;
            }
            if !enabled {
                continue;
            }
            if !inert_host_attributes(item) {
                return None;
            }
            if item.kind() == SyntaxKind::MACRO_CALL && !crate::macros::is_macro_rules(item) {
                pending.push(*by_call.get(&(file, item.id()))?);
            }
        }
    }
    Some(files)
}

fn inert_host_attributes(node: flux_syntax::SyntaxNode<'_>) -> bool {
    let attrs = crate::cfg::Attrs::new(node.children().filter_map(ast::Attr::cast));
    attrs.errors.is_empty()
        && attrs.iter().all(|attr| {
            matches!(
                attr_name(attr),
                Some(
                    "allow"
                        | "warn"
                        | "deny"
                        | "forbid"
                        | "expect"
                        | "doc"
                        | "cfg"
                        | "recursion_limit"
                        | "macro_export"
                        | "proc_macro"
                        | "proc_macro_attribute"
                        | "proc_macro_derive"
                        | "inline"
                        | "cold"
                        | "must_use"
                        | "deprecated"
                        | "repr"
                        | "no_std"
                        | "crate_name"
                )
            )
        })
}

/// Flux's target defaults differ from the rustc host's CPU features and
/// atomic widths. The compiled export list cannot select between two
/// declarations with the same name and different helper sets.
fn uses_rustc_target_cfg(node: flux_syntax::SyntaxNode<'_>) -> bool {
    node.children().filter_map(ast::Attr::cast).any(|attr| {
        matches!(attr_name(attr), Some("cfg" | "cfg_attr"))
            && attr.syntax().tokens().any(|token| {
                matches!(
                    token.text(),
                    "target_feature"
                        | "target_has_atomic"
                        | "target_has_atomic_equal_alignment"
                        | "target_has_atomic_primitive_alignment"
                )
            })
    })
}

fn function_declarations(function: ast::Fn<'_>) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut diagnostics = Vec::new();
    if !item_enabled(function.syntax(), &mut diagnostics) {
        return Ok(Vec::new());
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics[0].message.clone());
    }
    let Some(name) = function.name() else {
        return Ok(Vec::new());
    };
    let attrs = crate::cfg::Attrs::new(function.attrs());
    if let Some((_, message)) = attrs.errors.first() {
        return Err(message.clone());
    }
    let mut result = Vec::new();
    for attr in attrs.iter() {
        match attr_name(attr) {
            Some("proc_macro" | "proc_macro_attribute") => {
                result.push((name.text().to_owned(), Vec::new()));
            }
            Some("proc_macro_derive") => {
                let tree = attr
                    .meta()
                    .and_then(|m| m.token_tree())
                    .ok_or("missing proc-macro declaration arguments")?;
                let mut identifiers = tree
                    .syntax()
                    .tokens()
                    .filter(|t| t.kind() == SyntaxKind::IDENT)
                    .map(|t| t.text().to_owned());
                if let Some(name) = identifiers.next() {
                    if identifiers.next().is_some_and(|s| s != "attributes") {
                        return Err("unsupported proc-macro helper declaration".into());
                    }
                    result.push((name, identifiers.collect()));
                }
            }
            _ => {}
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
            proc_macro_declarations(&mut db, root, &[])
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
        let declarations = proc_macro_declarations(&mut db, root, &[]).unwrap();
        assert_eq!(declarations.get("Shared"), Some(&vec!["active".into()]));
        assert_eq!(declarations.get("identity"), Some(&Vec::new()));
    }

    #[test]
    fn generated_helpers_follow_root_expansions() {
        let mut db = SourceDb::empty();
        let root = db.add(
            "host.rs",
            r#"
            macro_rules! declare {
                ($name:ident, $helper:ident) => {
                    #[proc_macro_derive($name, attributes($helper))]
                    pub fn derive() {}
                };
            }
            macro_rules! wrap { ($helper:ident) => { declare!(Generated, $helper); }; }
            wrap!(active);
            #[cfg(any())] wrap!(inactive);
            "#,
        );
        let declarations = proc_macro_declarations(&mut db, root, &["Generated"]).unwrap();
        assert_eq!(declarations.get("Generated"), Some(&vec!["active".into()]));
    }

    #[test]
    fn generated_helpers_must_agree_between_host_profiles() {
        let mut db = SourceDb::empty();
        let root = db.add(
            "host.rs",
            r#"
            macro_rules! declare {
                ($helper:ident) => {
                    #[proc_macro_derive(Generated, attributes($helper))]
                    pub fn derive() {}
                };
            }
            #[cfg(debug_assertions)] declare!(debug_only);
            #[cfg(not(debug_assertions))] declare!(release_only);
            "#,
        );
        let declarations = proc_macro_declarations(&mut db, root, &["Generated"]).unwrap();
        assert!(!declarations.contains_key("Generated"));
    }

    #[test]
    fn unresolved_root_macro_can_change_later_exports() {
        let mut db = SourceDb::empty();
        let root = db.add(
            "host.rs",
            r#"
            macro_rules! declare {
                () => {
                    #[proc_macro_derive(Generated, attributes(old))]
                    pub fn derive() {}
                };
            }
            imported::redefine!();
            declare!();
            "#,
        );
        let declarations = proc_macro_declarations(&mut db, root, &["Generated"]).unwrap();
        assert!(!declarations.contains_key("Generated"));
    }

    #[test]
    fn generated_helpers_do_not_use_flux_target_defaults() {
        let mut db = SourceDb::empty();
        let root = db.add(
            "host.rs",
            r#"
            macro_rules! declare {
                ($helper:ident) => {
                    #[proc_macro_derive(Generated, attributes($helper))]
                    pub fn derive() {}
                };
            }
            #[cfg(target_has_atomic = "128")] declare!(wide);
            #[cfg(not(target_has_atomic = "128"))] declare!(narrow);
            "#,
        );
        let declarations = proc_macro_declarations(&mut db, root, &["Generated"]).unwrap();
        assert!(!declarations.contains_key("Generated"));
    }
}
