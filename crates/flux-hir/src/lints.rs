//! Scoped levels for compiler lints. Type errors and const-evaluation errors
//! never pass through this policy.
use crate::{FileId, ItemTree, ModuleId};
use flux_syntax::{
    NodeId,
    ast::{self, AstNode},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LintLevel {
    Allow,
    Warn,
    Deny,
    Forbid,
    Expect,
}
impl ItemTree {
    /// Effective level at this syntax identity, including external module and
    /// macro-call scopes. Lint lookup is needed only when a violation is found.
    pub fn lint_level(
        &self,
        module: ModuleId,
        file: FileId,
        node: NodeId,
        name: &str,
    ) -> LintLevel {
        self.lint_setting(module, file, node, name).0
    }
    fn lint_setting(
        &self,
        module: ModuleId,
        file: FileId,
        node: NodeId,
        name: &str,
    ) -> (
        LintLevel,
        Option<crate::Diagnostic>,
        Option<(FileId, flux_syntax::TextRange)>,
    ) {
        let mut sites = Vec::new();
        let mut modules = Vec::new();
        let mut at = Some(module);
        while let Some(id) = at {
            modules.push(id);
            at = self.modules[id].parent;
        }
        for id in modules.into_iter().rev() {
            let data = &self.modules[id];
            if let Some(&(file, node)) = self.module_declarations.get(&id) {
                self.lint_sites(file, node, &mut sites);
            }
            if let Some(source) = self.files.get(&data.file) {
                sites.push((data.file, source.parse.root().id()));
            }
        }
        self.lint_sites(file, node, &mut sites);
        let mut level = if name == "overflowing_literals" {
            LintLevel::Deny
        } else {
            LintLevel::Warn
        };
        let mut expectation = None;
        let mut warnings = LintLevel::Warn;
        let mut error = None;
        let mut seen = rustc_hash::FxHashSet::default();
        for (file, node) in sites {
            if !seen.insert((file, node)) {
                continue;
            }
            let Some(source) = self.files.get(&file) else {
                continue;
            };
            let syntax = source.parse.tree().node(node);
            let attrs = crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast));
            for attr in attrs.iter() {
                let next = match crate::item_tree::attr_name(attr) {
                    Some("allow") => LintLevel::Allow,
                    Some("warn") => LintLevel::Warn,
                    Some("deny") => LintLevel::Deny,
                    Some("forbid") => LintLevel::Forbid,
                    Some("expect") => LintLevel::Expect,
                    _ => continue,
                };
                let Some(tree) = attr.meta().and_then(|m| m.token_tree()) else {
                    continue;
                };
                let mut tokens = Vec::new();
                crate::macros::tt::tokens_of(tree.syntax(), &mut tokens);
                for (target, setting) in [(name, &mut level), ("warnings", &mut warnings)] {
                    if has_name(&tokens, target) {
                        if *setting == LintLevel::Forbid && next != LintLevel::Forbid {
                            error.get_or_insert(crate::Diagnostic {
                                file,
                                range: attr.syntax().text_range(),
                                message: format!(
                                    "{}({target}) incompatible with previous forbid",
                                    crate::item_tree::attr_name(attr).unwrap()
                                ),
                            });
                        } else {
                            *setting = next;
                            if target == name {
                                expectation = if next == LintLevel::Expect {
                                    Some((file, attr.syntax().text_range()))
                                } else {
                                    None
                                };
                            }
                        }
                    }
                }
            }
        }
        (
            if level == LintLevel::Warn {
                warnings
            } else {
                level
            },
            error,
            expectation,
        )
    }
    pub(crate) fn check_overflow_lint_overrides(&mut self) {
        let files = self.lint_files(Some(self.main_crate()));
        let mut found = Vec::new();
        for (file, module) in files {
            let Some(source) = self.files.get(&file) else {
                continue;
            };
            let mut enabled_nodes = rustc_hash::FxHashMap::default();
            for attr in source
                .parse
                .root()
                .descendants()
                .filter_map(ast::Attr::cast)
            {
                let attrs = crate::cfg::Attrs::new(std::iter::once(attr));
                if !attrs.iter().any(|expanded| {
                    if !matches!(
                        crate::item_tree::attr_name(expanded),
                        Some("allow" | "warn" | "deny" | "expect")
                    ) {
                        return false;
                    }
                    expanded
                        .meta()
                        .and_then(|m| m.token_tree())
                        .is_some_and(|tree| {
                            tree.syntax().tokens().any(|token| {
                                matches!(
                                    token.text().strip_prefix("r#").unwrap_or(token.text()),
                                    "overflowing_literals" | "warnings"
                                )
                            })
                        })
                }) {
                    continue;
                }
                if !enabled(attr.syntax(), &mut enabled_nodes) {
                    continue;
                }
                if let Some(parent) = attr.syntax().parent()
                    && let Some(error) = self
                        .lint_setting(module, file, parent.id(), "overflowing_literals")
                        .1
                {
                    found.push(error);
                }
            }
        }
        crate::Diagnostic::sort(&mut found);
        found.dedup_by(|a, b| a.file == b.file && a.range == b.range && a.message == b.message);
        self.diagnostics.extend(found);
    }
    fn lint_sites(&self, file: FileId, node: NodeId, sites: &mut Vec<(FileId, NodeId)>) {
        let Some(source) = self.files.get(&file) else {
            return;
        };
        if let Some(&(parent, call)) = self.expansion_sites.get(&file)
            && parent != file
        {
            self.lint_sites(parent, call, sites);
        } else if let Some(parent) = source.expansion_of
            && parent != file
            && let Some(origin) = self.files.get(&parent)
        {
            let call = source.parse.root().text_range();
            if let Some(site) = origin
                .parse
                .root()
                .descendants()
                .filter(|node| {
                    let r = node.text_range();
                    r.start() <= call.start() && call.end() <= r.end()
                })
                .min_by_key(|node| node.text_range().len())
            {
                ancestors(parent, site, sites);
            }
        }
        ancestors(file, source.parse.tree().node(node), sites);
    }
}
fn ancestors(file: FileId, node: flux_syntax::SyntaxNode<'_>, sites: &mut Vec<(FileId, NodeId)>) {
    let mut chain = Vec::new();
    let mut at = Some(node);
    while let Some(node) = at {
        chain.push((file, node.id()));
        at = node.parent();
    }
    sites.extend(chain.into_iter().rev());
}

/// Diagnostic accumulation for immutable signature queries. Clones snapshot
/// warnings, as item-tree loaded-crate edges do; compilations never share it.
#[derive(Debug, Default)]
pub(crate) struct LintWarnings(
    std::sync::Mutex<Vec<crate::Diagnostic>>,
    std::sync::Mutex<rustc_hash::FxHashSet<(FileId, flux_syntax::TextRange)>>,
);
impl Clone for LintWarnings {
    fn clone(&self) -> Self {
        Self(
            std::sync::Mutex::new(self.0.lock().unwrap().clone()),
            std::sync::Mutex::new(self.1.lock().unwrap().clone()),
        )
    }
}
impl ItemTree {
    pub fn lint_warnings(&self) -> Vec<crate::Diagnostic> {
        let mut warnings = self.lint_warnings.0.lock().unwrap().clone();
        crate::Diagnostic::sort(&mut warnings);
        warnings.dedup_by(|a, b| a.file == b.file && a.range == b.range && a.message == b.message);
        warnings
    }
    pub fn record_lint_warning(&self, warning: crate::Diagnostic) {
        self.lint_warnings.0.lock().unwrap().push(warning);
    }
    /// Signature constants retain a source range rather than an expression ID.
    pub fn lint_level_at(
        &self,
        module: ModuleId,
        file: FileId,
        range: flux_syntax::TextRange,
        name: &str,
    ) -> LintLevel {
        let (file, _) = self.source_span(file, range);
        let Some(source) = self.files.get(&file) else {
            return LintLevel::Deny;
        };
        let node = source
            .parse
            .root()
            .descendants()
            .filter(|node| {
                let r = node.text_range();
                r.start() <= range.start() && range.end() <= r.end()
            })
            .min_by_key(|node| node.text_range().len())
            .unwrap_or_else(|| source.parse.root());
        self.lint_level(module, file, node.id(), name)
    }
}

impl ItemTree {
    pub fn fulfill_overflow_expectation(&self, module: ModuleId, file: FileId, node: NodeId) {
        if let Some(origin) = self
            .lint_setting(module, file, node, "overflowing_literals")
            .2
        {
            self.lint_warnings.1.lock().unwrap().insert(origin);
        }
    }
    pub fn fulfill_overflow_expectation_at(
        &self,
        module: ModuleId,
        file: FileId,
        range: flux_syntax::TextRange,
    ) {
        if let Some(source) = self.files.get(&file)
            && let Some(node) = source
                .parse
                .root()
                .descendants()
                .filter(|node| {
                    let r = node.text_range();
                    r.start() <= range.start() && range.end() <= r.end()
                })
                .min_by_key(|node| node.text_range().len())
        {
            self.fulfill_overflow_expectation(module, file, node.id());
        }
    }
    pub fn overflow_expectation_diagnostics(
        &self,
    ) -> (Vec<crate::Diagnostic>, Vec<crate::Diagnostic>) {
        let files = self.lint_files(None);
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        for (file, module) in files {
            let Some(source) = self.files.get(&file) else {
                continue;
            };
            let mut enabled_nodes = rustc_hash::FxHashMap::default();
            for raw in source
                .parse
                .root()
                .descendants()
                .filter_map(ast::Attr::cast)
            {
                let attrs = crate::cfg::Attrs::new(std::iter::once(raw));
                for attr in attrs.iter() {
                    if crate::item_tree::attr_name(attr) != Some("expect") {
                        continue;
                    }
                    let Some(tree) = attr.meta().and_then(|m| m.token_tree()) else {
                        continue;
                    };
                    let mut tokens = Vec::new();
                    crate::macros::tt::tokens_of(tree.syntax(), &mut tokens);
                    if !has_name(&tokens, "overflowing_literals") {
                        continue;
                    }
                    if !enabled(raw.syntax(), &mut enabled_nodes) {
                        continue;
                    }
                    let range = attr.syntax().text_range();
                    if self
                        .lint_warnings
                        .1
                        .lock()
                        .unwrap()
                        .contains(&(file, range))
                    {
                        continue;
                    }
                    let parent = raw.syntax().parent().unwrap_or_else(|| source.parse.root());
                    let diagnostic = crate::Diagnostic {
                        file,
                        range,
                        message: "this lint expectation is unfulfilled".into(),
                    };
                    match self.lint_level(
                        module,
                        file,
                        parent.id(),
                        "unfulfilled_lint_expectations",
                    ) {
                        LintLevel::Allow | LintLevel::Expect => {}
                        LintLevel::Warn => warnings.push(diagnostic),
                        LintLevel::Deny | LintLevel::Forbid => errors.push(diagnostic),
                    }
                }
            }
        }
        (errors, warnings)
    }
}

fn enabled(
    node: flux_syntax::SyntaxNode<'_>,
    cache: &mut rustc_hash::FxHashMap<NodeId, bool>,
) -> bool {
    let mut pending = Vec::new();
    let mut at = node.parent();
    let mut enabled = true;
    while let Some(node) = at {
        if let Some(&known) = cache.get(&node.id()) {
            enabled = known;
            break;
        }
        pending.push(node);
        at = node.parent();
    }
    for node in pending.into_iter().rev() {
        enabled = enabled && crate::item_tree::item_enabled(node, &mut Vec::new());
        cache.insert(node.id(), enabled);
    }
    enabled
}

impl ItemTree {
    fn lint_files(
        &self,
        krate: Option<crate::CrateId>,
    ) -> std::collections::BTreeMap<FileId, ModuleId> {
        let mut files = std::collections::BTreeMap::new();
        let included = |owner| {
            krate.is_none_or(|wanted| wanted == owner) && !self.artifact_files.contains_key(&owner)
        };
        for (module, data) in self.modules.iter() {
            if included(data.krate) {
                files.entry(data.file).or_insert(module);
                if let Some(&(file, _)) = self.module_declarations.get(&module) {
                    files.entry(file).or_insert(data.parent.unwrap_or(module));
                }
            }
        }
        for (_, data) in self.functions.iter() {
            if included(self.modules[data.loc.module].krate) {
                files.entry(data.loc.file).or_insert(data.loc.module);
            }
        }
        for (_, data) in self.consts.iter() {
            if included(self.modules[data.loc.module].krate) {
                files.entry(data.loc.file).or_insert(data.loc.module);
            }
        }
        loop {
            let before = files.len();
            for ((parent, _), expansion) in &self.body_expansions {
                if let Some(&module) = files.get(parent) {
                    files.entry(expansion.file).or_insert(module);
                }
            }
            if files.len() == before {
                break;
            }
        }
        files
    }
}

fn has_name(tokens: &[crate::macros::tt::Tok], name: &str) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token
            .text
            .as_str()
            .strip_prefix("r#")
            .unwrap_or(token.text.as_str())
            == name
            && (index == 0
                || !matches!(
                    tokens[index - 1].kind,
                    flux_syntax::SyntaxKind::COLON2 | flux_syntax::SyntaxKind::COLON
                ))
            && tokens.get(index + 1).is_none_or(|t| {
                !matches!(
                    t.kind,
                    flux_syntax::SyntaxKind::COLON2 | flux_syntax::SyntaxKind::COLON
                )
            })
    })
}
