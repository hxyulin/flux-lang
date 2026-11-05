//! Macros: `macro_rules!` definitions, the macros the compiler
//! implements, and expanding a call into a syntax tree of its own.
//!
//! A call's tokens and the tokens it expands to are [`Tok`]s, each with
//! where it came from: its range in the file being lowered, and a
//! *group* of tokens in order there (the file's own, or one macro
//! definition's in that file), or none, for tokens a macro makes or
//! whose definition is in another file. The expansion is parsed as the
//! call's position wants (an expression, statements, items, ...), and
//! the tree's ranges are those origins: a node of one group's tokens in
//! order is where they are, and one mixing them is at the call, as
//! rustc's diagnostics point into local macros' definitions and at
//! calls of others. So the HIR made from an expansion has ranges in the
//! file like any other.
//!
//! Hygiene is rustc's for `macro_rules!` (mixed-site): each expansion
//! gives the tokens its definition wrote a new *context*, a number, and
//! local variables and labels are found by name and context, so a
//! macro's locals and its caller's never see each other. Items are found
//! by name alone.

pub(crate) mod builtin;
pub(crate) mod inline_asm;
pub(crate) mod mbe;
pub(crate) mod tt;

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use flux_arena::Idx;
use flux_intern::{Symbol, kw, sym};
use flux_syntax::ast::{self, AstNode};
use flux_syntax::parser::{EXPANSION_IGNORES, Fragment};
use flux_syntax::{Parse, SyntaxKind, SyntaxNode, T, TextRange, TokenOrigin};
use rustc_hash::FxHashMap;

pub use builtin::Builtin;
use builtin::BuiltinCx;
use mbe::{CallInput, MatchError};
use tt::{Tok, tokens_of};

use crate::{CrateId, Diagnostic, FileId, ItemTree, ModuleId};

pub type MacroId = Idx<MacroDef>;
pub type MacroScopeId = Idx<MacroScope>;

/// A macro: a `macro_rules!` definition, or one the compiler implements.
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub name: Symbol,
    pub kind: MacroKind,
    pub krate: CrateId,
    /// The file it is written in, not an expansion's.
    pub file: FileId,
    pub name_range: TextRange,
    /// The group of its tokens, in expansions in its file.
    pub(crate) group: u32,
}

#[derive(Debug, Clone)]
pub enum MacroKind {
    Rules(Arc<mbe::Rules>),
    Builtin(Builtin),
    ProcMacro(Arc<crate::ProcMacro>),
}

/// One `macro_rules!` definition in textual scope, after the ones in
/// `parent`: macros are in scope from their definition to the end of
/// the module or block they are in (or of its parent, for a module with
/// `#[macro_use]`).
#[derive(Debug, Clone, Copy)]
pub struct MacroScope {
    pub parent: Option<MacroScopeId>,
    pub name: Symbol,
    pub def: MacroId,
    /// How many definitions are before it, `parent`'s included.
    pub(crate) depth: u32,
    /// An ancestor further up, for finding one at a given depth in
    /// logarithmic time (skew-binary jump pointers).
    pub(crate) jump: Option<MacroScopeId>,
}

impl ItemTree {
    /// Defines `def`, named `name`, after the macros in `parent`.
    pub(crate) fn push_macro_scope(
        &mut self,
        parent: Option<MacroScopeId>,
        name: Symbol,
        def: MacroId,
    ) -> MacroScopeId {
        let (depth, jump) = match parent {
            None => (0, None),
            Some(p) => {
                let pd = &self.macro_scopes[p];
                let jump = match pd.jump {
                    Some(j) => {
                        let jd = &self.macro_scopes[j];
                        match jd.jump {
                            Some(jj)
                                if pd.depth - jd.depth
                                    == jd.depth - self.macro_scopes[jj].depth =>
                            {
                                Some(jj)
                            }
                            _ => Some(p),
                        }
                    }
                    None => Some(p),
                };
                (pd.depth + 1, jump)
            }
        };
        let id = self.macro_scopes.alloc(MacroScope {
            parent,
            name,
            def,
            depth,
            jump,
        });
        self.macro_names.entry(name).or_default().push(id);
        id
    }

    /// The macro named `name` in textual scope at `scope`: the latest
    /// definition of the name that is `scope` or one of its ancestors.
    pub(crate) fn textual_macro(
        &self,
        scope: Option<MacroScopeId>,
        name: Symbol,
    ) -> Option<MacroId> {
        let scope = scope?;
        let candidates = self.macro_names.get(&name)?;
        let depth = self.macro_scopes[scope].depth;
        for &c in candidates.iter().rev() {
            let cd = self.macro_scopes[c].depth;
            if cd > depth {
                continue;
            }
            let mut v = scope;
            while self.macro_scopes[v].depth > cd {
                let data = &self.macro_scopes[v];
                v = match data.jump {
                    Some(j) if self.macro_scopes[j].depth >= cd => j,
                    _ => data.parent.expect("deeper than an ancestor"),
                };
            }
            if v == c {
                return Some(self.macro_scopes[c].def);
            }
        }
        None
    }
}

/// How deep expansions may nest: rustc's default `recursion_limit`.
pub(crate) const RECURSION_LIMIT: u32 = 128;

static NEXT_CTXT: AtomicU32 = AtomicU32::new(1);
static NEXT_GROUP: AtomicU32 = AtomicU32::new(tt::SOURCE_GROUP + 1);

/// A hygiene context no other expansion has.
pub(crate) fn fresh_ctxt() -> u32 {
    let ctxt = NEXT_CTXT.fetch_add(1, Ordering::Relaxed);
    assert!(ctxt < DEF_CRATE_CTXT, "too many macro hygiene contexts");
    ctxt
}

// Only compiler-produced `crate` tokens carry these contexts. Reserving the
// high bit preserves definition-crate identity through token capture/replay,
// including macro definitions generated by another expansion.
const DEF_CRATE_CTXT: u32 = 1 << 31;
pub(crate) fn definition_crate(ctxt: u32) -> Option<CrateId> {
    (ctxt & DEF_CRATE_CTXT != 0).then_some(CrateId(ctxt & !DEF_CRATE_CTXT))
}

/// A token group for a new macro definition.
pub(crate) fn fresh_group() -> u32 {
    NEXT_GROUP.fetch_add(1, Ordering::Relaxed)
}

/// Macros `std` defines and `core` does not, which flux has no `std` for.
const STD_MACROS: &[&str] = &[
    "format",
    "vec",
    "thread_local",
    "is_x86_feature_detected",
    "is_aarch64_feature_detected",
];

/// `std`'s macros that flux's `core` defines, as if in `std`'s prelude:
/// named alone or through `std`, never through `core`.
pub(crate) const CORE_STD_MACROS: &[&str] = &["print", "println", "eprint", "eprintln", "dbg"];

impl MacroDef {
    /// Reads `macro_rules! name { rules }` from `call`, a macro call
    /// whose path is `macro_rules`, in `krate` at `file`. `None` if it
    /// has no name, which the parser reported.
    pub(crate) fn from_rules(
        call: SyntaxNode<'_>,
        krate: CrateId,
        file: FileId,
        edition: flux_syntax::Edition,
        builtin: bool,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<MacroDef> {
        let name = call.children().find_map(ast::Name::cast)?;
        let tt = call.children().find_map(ast::TokenTree::cast);
        let name_range = name.syntax().text_range();
        let name_sym = Symbol::intern(name.text());
        let kind = if builtin {
            MacroKind::Builtin(Builtin::from_name(name.text()))
        } else {
            let mut toks = Vec::new();
            if let Some(tt) = tt {
                tokens_of(tt.syntax(), &mut toks);
            }
            let inner = strip_delimiters(&toks);
            let edition = call
                .tokens()
                .find(|t| !t.kind().is_trivia())
                .map_or(edition, |t| t.edition());
            let (mut rules, errors) = mbe::parse_rules(inner.to_vec(), name_range, edition);
            rules.local_inner_macros = call.children().filter_map(ast::Attr::cast).any(|attr| {
                crate::item_tree::attr_name(attr) == Some("macro_export")
                    && attr
                        .meta()
                        .and_then(|m| m.token_tree())
                        .is_some_and(|tree| {
                            tree.syntax()
                                .tokens()
                                .any(|token| token.text() == "local_inner_macros")
                        })
            });
            for (range, message) in errors {
                diagnostics.push(Diagnostic::new(range, message));
            }
            MacroKind::Rules(Arc::new(rules))
        };
        Some(MacroDef {
            name: name_sym,
            kind,
            krate,
            file,
            name_range,
            group: fresh_group(),
        })
    }
}

/// `toks` without the delimiters around them, if they have them.
pub(crate) fn strip_delimiters(toks: &[Tok]) -> &[Tok] {
    match toks {
        [open, inner @ .., close] if tt::is_open(open.kind) && tt::is_close(close.kind) => inner,
        [open, inner @ ..] if tt::is_open(open.kind) => inner,
        _ => toks,
    }
}

/// Whether `node`, a macro call, is `macro_rules! name { .. }`.
pub(crate) fn is_macro_rules(node: SyntaxNode<'_>) -> bool {
    node.children().any(|c| c.kind() == SyntaxKind::NAME)
        && node
            .children()
            .find_map(ast::Path::cast)
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .is_some_and(|n| n.text() == "macro_rules")
}

/// Where a call is, for what expanding it depends on.
pub(crate) struct CallSite<'a> {
    pub items: &'a ItemTree,
    pub krate: CrateId,
    pub module: ModuleId,
    pub scope: Option<MacroScopeId>,
    /// The file the call is written in, not an expansion's.
    pub real_file: FileId,
    /// Loading is supplied by the source layer, never by token formatting.
    pub read_text: fn(&crate::source::SourceFile, &str) -> Result<String, String>,
}

/// A call's expansion: its tree, and the hygiene contexts it made, each
/// with the context its tokens had in the definition.
pub(crate) struct Expansion {
    pub parse: Parse,
    /// Physical source inclusion, whose ranges belong to this path.
    pub included_path: Option<std::path::PathBuf>,
    pub ctxts: Vec<(u32, u32)>,
}

/// The macro a call names, if it names one: `local` is looked up in
/// first, from the end (macros defined in the body being lowered),
/// then `scope`'s textual scope, then `module`'s exported macros and
/// `core`'s.
pub(crate) fn resolve<'a, L>(
    items: &'a ItemTree,
    path: ast::Path<'_>,
    local: &'a [(Symbol, L)],
    scope: Option<MacroScopeId>,
    module: ModuleId,
) -> Result<Resolved<'a, L>, Diagnostic> {
    let range = path.syntax().text_range();
    let (global, segments) =
        macro_path(path).ok_or_else(|| Diagnostic::new(range, "expected a macro name"))?;
    let name = *segments.last().expect("a path has a segment");
    let not_found = || Diagnostic::new(range, format!("cannot find macro `{name}` in this scope"));
    let krate = items.modules[module].krate;
    let own = &items.crates[krate.0 as usize];
    let core_crate = items
        .crates
        .iter()
        .position(|c| c.is_core)
        .map(|i| CrateId(i as u32));
    let exported = |k: CrateId| {
        items
            .exported_macros
            .get(&(k, name))
            .copied()
            .filter(|&id| {
                k == krate || !matches!(items.macros[id].kind, MacroKind::Builtin(Builtin::Asm))
            })
    };
    // A macro by path, through the def map: what modules export or import.
    let by_path = || {
        let lowered = crate::item_tree::lower_path(path, &mut Vec::new())?;
        match resolve_predecessor(items, module, &lowered, crate::def_map::Ns::Macros)? {
            crate::def_map::Resolved::Def(crate::def_map::Def::Macro(id)) => Some(id),
            _ => None,
        }
    };
    // `std::vec!` and `std::format!` are `alloc`'s.
    let std_macro =
        CORE_STD_MACROS.contains(&name.as_str()) || matches!(name.as_str(), "vec" | "format");
    let alloc_crate = items
        .crates
        .iter()
        .position(|c| c.is_core && c.name == sym::alloc)
        .map(|i| CrateId(i as u32));
    if own.no_std
        && segments.first() == Some(&sym::std)
        && !own.externs.contains(&sym::std)
        && (global
            || items.def_map.scopes.len() < items.modules.len()
            || items
                .lookup(module, sym::std, crate::def_map::Ns::Types)
                .is_none())
    {
        return Err(not_found());
    }
    if segments.len() > 1
        && !(segments.first() == Some(&sym::core) && std_macro)
        && let Some(id) = by_path()
    {
        return Ok(Resolved::Def(id));
    }
    match (global, segments.as_slice()) {
        (false, [_]) => {
            if let Some((_, l)) = local.iter().rev().find(|(n, _)| *n == name) {
                return Ok(Resolved::Local(l));
            }
            if let Some(def) = items.textual_macro(scope, name) {
                return Ok(Resolved::Def(def));
            }
            if let Some(id) = by_path() {
                return Ok(Resolved::Def(id));
            }
            let at_root = items.modules[items.named_module(module)].parent.is_none();
            if at_root && let Some(id) = exported(krate) {
                return Ok(Resolved::Def(id));
            }
            if let Some(&(_, id)) = own.macro_use.iter().rev().find(|&&(n, _)| n == name) {
                return Ok(Resolved::Def(id));
            }
            if let Some(&id) = items.macro_prelude.get(&name)
                && (!own.no_std
                    || (items.crates[items.macros[id].krate.0 as usize].name == sym::core
                        && !CORE_STD_MACROS.contains(&name.as_str())))
            {
                return Ok(Resolved::Def(id));
            }
            if STD_MACROS.contains(&name.as_str()) {
                return Err(Diagnostic::new(
                    range,
                    format!(
                        "`{name}!` is from `std`, which is not supported yet; flux has only `core`"
                    ),
                ));
            }
            Err(not_found())
        }
        (false, [first, _]) if *first == kw::Crate || *first == kw::SelfLower => {
            let at_root = items.modules[items.named_module(module)].parent.is_none();
            if *first == kw::SelfLower && !at_root {
                return Err(not_found());
            }
            exported(krate).map(Resolved::Def).ok_or_else(not_found)
        }
        (_, [first, _]) if *first == sym::core && std_macro => Err(Diagnostic::new(
            range,
            format!("could not find `{name}` in `core`"),
        )),
        (_, [first, _]) if *first == sym::core => core_crate
            .and_then(exported)
            .map(Resolved::Def)
            .ok_or_else(not_found),
        (_, [first, _]) if *first == sym::std && std_macro => core_crate
            .and_then(exported)
            .or_else(|| items.macro_prelude.get(&name).copied())
            .map(Resolved::Def)
            .ok_or_else(not_found),
        (_, [first, _]) if *first == sym::alloc => alloc_crate
            .and_then(exported)
            .map(Resolved::Def)
            .ok_or_else(not_found),
        _ => Err(not_found()),
    }
}

/// Resolves a completed dependency while the current crate's scopes are still
/// being collected. Imports keep their defining module and are followed once.
pub(crate) fn resolve_predecessor(
    items: &ItemTree,
    module: ModuleId,
    path: &crate::Path,
    ns: crate::def_map::Ns,
) -> Option<crate::def_map::Resolved> {
    let krate = items.modules[module].krate;
    let mut lowered = path.clone();
    // Current-crate item collection can call a predecessor's macro before
    // current scopes exist. Resolve through that crate's completed scope.
    if items.def_map.scopes.len() < items.modules.len() {
        let mut lookup_module = module;
        let mut followed_imports = Vec::new();
        for _ in 0..RECURSION_LIMIT {
            if !matches!(
                lowered.anchor,
                crate::PathAnchor::Plain | crate::PathAnchor::Global
            ) {
                break;
            }
            let first = *lowered.segments.first()?;
            let mut current = lookup_module;
            let mut import = None;
            if lowered.anchor == crate::PathAnchor::Plain {
                loop {
                    import = items.uses.iter().enumerate().rev().find(|(index, u)| !followed_imports.contains(index) && u.module == current
                            && matches!(u.kind, crate::item_tree::UseKind::Named(alias) | crate::item_tree::UseKind::Module(alias) if alias == first));
                    if import.is_some() {
                        break;
                    }
                    let data = &items.modules[current];
                    match (data.block, data.parent) {
                        (Some(_), Some(parent)) => current = parent,
                        _ => break,
                    }
                }
            }
            if let Some((index, import)) = import {
                // An import's own source path does not resolve through
                // that same binding again (`use cfg_if::cfg_if`). Keep
                // walking predecessor aliases without cycling on them.
                followed_imports.push(index);
                let mut path = import.path.clone();
                path.segments = path
                    .segments
                    .iter()
                    .chain(lowered.segments[1..].iter())
                    .copied()
                    .collect();
                path.ranges = path
                    .ranges
                    .iter()
                    .chain(lowered.ranges.get(1..).unwrap_or_default())
                    .copied()
                    .collect();
                path.args = path
                    .args
                    .iter()
                    .chain(lowered.args.get(1..).unwrap_or_default())
                    .cloned()
                    .collect();
                lowered = path;
                lookup_module = import.module;
                continue;
            }
            // A local module shadows the implicit extern prelude. Its
            // current-crate scope is audited after item collection.
            if lowered.anchor == crate::PathAnchor::Plain
                && items.modules.iter().any(|(_, m)| {
                    m.parent == Some(items.named_module(lookup_module)) && m.name == first
                })
            {
                return None;
            }
            let target = items.modules[items.loaded_extern_crate(first, lookup_module)?].krate;
            lowered.anchor = crate::PathAnchor::DefinitionCrate(target);
            lowered.segments = lowered.segments[1..].into();
            lowered.ranges = lowered.ranges.get(1..).unwrap_or_default().into();
            lowered.args = lowered.args.get(1..).unwrap_or_default().into();
            break;
        }
        if !matches!(lowered.anchor, crate::PathAnchor::DefinitionCrate(target) if target != krate)
        {
            return None;
        }
    }
    items.resolve(module, &lowered, ns).ok()
}

/// What a macro call's path names.
pub(crate) enum Resolved<'a, L> {
    Def(MacroId),
    Local(&'a L),
}

/// A macro call's path: whether it starts with `::`, and its names.
fn macro_path(path: ast::Path<'_>) -> Option<(bool, Vec<Symbol>)> {
    let mut segments = Vec::new();
    let mut current = Some(path);
    let mut global = false;
    while let Some(p) = current {
        let segment = p.segment()?;
        if segment.generic_arg_list().is_some() {
            return None;
        }
        let name = segment.name_ref()?;
        if let Some(colons) = segment.colon2_token()
            && colons.raw_range().start() < name.syntax().raw_range().start()
        {
            global = true;
        }
        segments.push(Symbol::intern(name.text()));
        current = p.qualifier();
    }
    segments.reverse();
    Some((global, segments))
}

/// Expands a call of `def`: `call` is the call's node, whose token tree
/// is the macro's input, parsed as `fragment`.
pub(crate) fn expand(
    def: &MacroDef,
    call: SyntaxNode<'_>,
    site: &CallSite<'_>,
    fragment: Fragment,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expansion> {
    expand_at_depth(def, call, site, fragment, diagnostics, 0)
}

fn expand_at_depth(
    def: &MacroDef,
    call: SyntaxNode<'_>,
    site: &CallSite<'_>,
    fragment: Fragment,
    diagnostics: &mut Vec<Diagnostic>,
    depth: u32,
) -> Option<Expansion> {
    let call_range = call_range(call);
    if depth as usize >= site.items.crates[site.krate.0 as usize].recursion_limit {
        diagnostics.push(Diagnostic::new(
            call_range,
            "recursion limit reached during eager macro expansion",
        ));
        return None;
    }
    let mut toks = Vec::new();
    let tt = call
        .children()
        .find(|c| c.kind() == SyntaxKind::TOKEN_TREE)?;
    match def.kind {
        MacroKind::ProcMacro(_) => tt::proc_tokens_ignoring(tt, &[], &mut toks),
        MacroKind::Rules(_) => tt::rules_call_tokens(tt, &mut toks),
        _ => tokens_of(tt, &mut toks),
    }
    let inner = strip_delimiters(&toks);
    // Where the input ends: its closing delimiter, or the whole call if
    // it is empty.
    let eof_range = match (inner.is_empty(), toks.last()) {
        (false, Some(close)) if tt::is_close(close.kind) => close.range(),
        _ => call_range,
    };
    let mut ctxts = Vec::new();
    let output = match &def.kind {
        MacroKind::ProcMacro(host) => {
            if host.kind != crate::ProcMacroKind::Function {
                diagnostics.push(Diagnostic::new(
                    call_range,
                    "expected a function-like macro",
                ));
                return None;
            }
            expand_proc(host, inner, None, site, call_range, &mut ctxts, diagnostics)?
        }
        MacroKind::Rules(rules) => {
            if rules.invalid {
                return None;
            }
            let input = CallInput::new(inner, eof_range);
            let mut best = None;
            let mut matched = None;
            for rule in 0..rules.len() {
                match rules.match_rule(rule, &input, def.name) {
                    Ok(bindings) => {
                        matched = Some((rule, bindings));
                        break;
                    }
                    Err(MatchError::Failure { at }) => {
                        best = Some(best.map_or(at, |b: usize| b.max(at)));
                    }
                    Err(MatchError::Error { range, message }) => {
                        diagnostics.push(Diagnostic::new(range, message));
                        return None;
                    }
                }
            }
            let Some((rule, bindings)) = matched else {
                let at = best.unwrap_or(0);
                let (range, message) = if at >= input.glued.len() {
                    (eof_range, "unexpected end of macro invocation".to_owned())
                } else {
                    let g = input.glued[at];
                    let tok = &inner[g.raw as usize];
                    (
                        input.range(at),
                        format!(
                            "no rules expected {}",
                            describe_token(tok.kind, tok.text, inner, g)
                        ),
                    )
                };
                diagnostics.push(Diagnostic::new(range, message));
                return None;
            };
            let in_file = def.file == site.real_file;
            let marks: RefCell<FxHashMap<u32, u32>> = RefCell::default();
            let ctxts_cell = RefCell::new(&mut ctxts);
            let mark = |t: &Tok| -> Tok {
                let ctxt = if t.kind == T![crate] && definition_crate(t.origin.ctxt).is_some() {
                    t.origin.ctxt
                } else {
                    *marks.borrow_mut().entry(t.origin.ctxt).or_insert_with(|| {
                        let new = fresh_ctxt();
                        ctxts_cell.borrow_mut().push((new, t.origin.ctxt));
                        new
                    })
                };
                let origin = if in_file && t.origin.group != TokenOrigin::SYNTHETIC {
                    TokenOrigin {
                        range: t.origin.range,
                        group: def.group,
                        ctxt,
                    }
                } else {
                    TokenOrigin {
                        range: call_range,
                        group: def.group,
                        ctxt,
                    }
                };
                Tok { origin, ..*t }
            };
            let crate_tokens = |t: &Tok, out: &mut Vec<Tok>| {
                let mut t = mark(t);
                t.origin.ctxt = DEF_CRATE_CTXT | def.krate.0;
                out.push(Tok {
                    kind: T![crate],
                    text: kw::Crate,
                    ..t
                });
            };
            let result = rules.transcribe(
                rule,
                &input,
                &bindings,
                &mut |t, out| out.push(mark(t)),
                &mut |t, out| crate_tokens(t, out),
            );
            match result {
                Ok(out) => out,
                Err((range, message)) => {
                    diagnostics.push(Diagnostic::new(range, message));
                    return None;
                }
            }
        }
        &MacroKind::Builtin(builtin) => {
            // `panic!` and `unreachable!` pass their first argument to
            // `format_args!`, or in editions 2015 and 2018 take a single
            // literal, which a macro such as `concat!` may produce.
            let format_first = matches!(
                builtin,
                Builtin::FormatArgs
                    | Builtin::ConstFormatArgs
                    | Builtin::FormatArgsNl
                    | Builtin::Panic
                    | Builtin::Unreachable
            );
            let capture_format_variables = !format_first
                || inner
                    .first()
                    .is_some_and(|token| token.kind == SyntaxKind::STRING);
            let eager;
            let inner = if format_first
                || matches!(
                    builtin,
                    Builtin::Concat
                        | Builtin::Include
                        | Builtin::IncludeStr
                        | Builtin::IncludeBytes
                        | Builtin::Env
                        | Builtin::OptionEnv
                ) {
                eager = eager_arguments(inner, site, diagnostics, depth + 1, format_first)?;
                &eager[..]
            } else {
                inner
            };
            let ctxt = fresh_ctxt();
            ctxts.push((ctxt, 0));
            // Builtin paths retain the physical embedded core identity.
            let location = || {
                let file = site.items.files.get(&site.real_file);
                let Some(file) = file else {
                    return (1, 1, String::new());
                };
                let text = file.text();
                let offset =
                    (call_range.start().saturating_sub(file.range_base) as usize).min(text.len());
                let before = &text[..offset];
                let line = before.matches('\n').count() as u32 + 1;
                let line_start = before.rfind('\n').map_or(0, |i| i + 1);
                let column = before[line_start..].chars().count() as u32 + 1;
                (line, column, file.path.display().to_string())
            };
            let module_path = || {
                let items = site.items;
                let mut names = Vec::new();
                let mut m = Some(items.named_module(site.module));
                while let Some(id) = m {
                    let module = &items.modules[id];
                    if module.block.is_none() {
                        names.push(module.name.to_string());
                    }
                    m = module.parent;
                }
                names.reverse();
                names.join("::")
            };
            // Rust resolves macro-expanded include paths at the invocation's
            // source file. A retained include itself owns its physical file,
            // so nested includes use the included file's directory.
            let source = site.items.files.get(&site.real_file);
            let include_str = |path: &str| {
                let source = source.ok_or_else(|| "include source file is unknown".to_owned())?;
                let text = (site.read_text)(source, path)?;
                let physical = crate::source::SourceDb::include_path(source, path)?;
                site.items.crates[site.krate.0 as usize]
                    .record_file_input(physical, text.as_bytes())?;
                Ok(text)
            };
            let read_bytes = |path: &str| {
                let source = source.ok_or_else(|| "include source file is unknown".to_owned())?;
                let bytes = crate::source::SourceDb::read_include_bytes(source, path)?;
                let physical = crate::source::SourceDb::include_path(source, path)?;
                site.items.crates[site.krate.0 as usize].record_file_input(physical, &bytes)?;
                Ok(bytes)
            };
            let track_environment = |name: &str, value: Option<&str>| {
                site.items.crates[site.krate.0 as usize].record_environment_input(name, value)
            };
            let edition = call
                .children()
                .find_map(ast::Path::cast)
                .and_then(|p| p.segment())
                .and_then(|s| s.name_ref())
                .and_then(|name| name.syntax().tokens().next())
                .map_or(site.items.edition(site.module), |t| t.edition());
            let cx = BuiltinCx {
                name: def.name,
                capture_format_variables,
                environment: site.items.crates[site.krate.0 as usize]
                    .environment
                    .as_deref(),
                edition,
                cfg: site.items.crates[site.krate.0 as usize].cfg.as_deref(),
                toks: inner,
                call: call_range,
                ctxt,
                core: CrateId(0),
                location: &location,
                module_path: &module_path,
                include_str: &include_str,
                read_bytes: &read_bytes,
                track_environment: &track_environment,
            };
            if builtin == Builtin::Include {
                let result = (|| {
                    if !matches!(fragment, Fragment::Expr | Fragment::Stmts | Fragment::Items) {
                        return Err((
                            call_range,
                            "include! requires an expression or item position".into(),
                        ));
                    }
                    let filename = builtin::filename(&cx)?;
                    let source = source
                        .ok_or_else(|| (call_range, "include source file is unknown".to_owned()))?;
                    let path = crate::source::SourceDb::include_path(source, &filename)
                        .map_err(|e| (call_range, e))?;
                    let text = include_str(&filename).map_err(|e| (call_range, e))?;
                    let edition = site.items.edition(site.module);
                    let parse = if fragment == Fragment::Stmts {
                        let expression = flux_syntax::parse_included_source(
                            text.clone().into_boxed_str(),
                            Fragment::Expr,
                            edition,
                        );
                        if expression.errors().is_empty() {
                            flux_syntax::parse_included_source(
                                text.into_boxed_str(),
                                fragment,
                                edition,
                            )
                        } else {
                            expression
                        }
                    } else {
                        flux_syntax::parse_included_source(text.into_boxed_str(), fragment, edition)
                    };
                    Ok(Expansion {
                        parse: parse
                            .with_cfg_options(site.items.crates[site.krate.0 as usize].cfg.clone()),
                        included_path: Some(path),
                        ctxts: Vec::new(),
                    })
                })();
                return match result {
                    Ok(expansion) => Some(expansion),
                    Err((range, message)) => {
                        diagnostics.push(Diagnostic::new(range, message));
                        None
                    }
                };
            }
            match builtin::expand(builtin, &cx) {
                Ok(out) => out,
                Err((range, message)) => {
                    diagnostics.push(Diagnostic::new(range, message));
                    return None;
                }
            }
        }
    };
    let mut starts = Vec::with_capacity(output.len());
    let text = tt::write_text(&output, &mut starts);
    let origins: Vec<(u32, TokenOrigin)> = starts
        .iter()
        .zip(&output)
        .map(|(&s, t)| (s, t.origin))
        .collect();
    let kinds: Vec<_> = starts
        .iter()
        .zip(&output)
        .map(|(&start, token)| (start, token.kind))
        .collect();
    let editions: Vec<_> = starts
        .iter()
        .zip(&output)
        .map(|(&start, token)| (start, token.edition))
        .collect();
    let parse = flux_syntax::parse_expansion_with_token_kinds(
        text.into(),
        &origins,
        &kinds,
        call_range,
        fragment,
        site.items.edition(site.module),
    )
    .with_token_editions(&editions)
    .with_cfg_options(site.items.crates[site.krate.0 as usize].cfg.clone());
    for error in parse.errors() {
        let message = if error.message == EXPANSION_IGNORES {
            let tree = parse.tree();
            let ignored = tree
                .root()
                .children()
                .filter(|c| c.kind() == SyntaxKind::ERROR)
                .last()
                .and_then(|e| e.tokens().find(|t| !t.kind().is_trivia()));
            match ignored {
                Some(t) => format!(
                    "macro expansion ignores {} and any tokens following",
                    describe_kind(t.kind(), t.text())
                ),
                None => "macro expansion ignores tokens".to_owned(),
            }
        } else {
            error.message.clone()
        };
        diagnostics.push(Diagnostic::new(error.range, message));
    }
    if !parse.errors().is_empty() {
        return None;
    }
    Some(Expansion {
        parse,
        ctxts,
        included_path: None,
    })
}

/// Eager builtins expand nested macro arguments before interpreting literals.
/// Parse each argument as an expression, preserving token origins for resolution.
fn eager_arguments(
    toks: &[Tok],
    site: &CallSite<'_>,
    diagnostics: &mut Vec<Diagnostic>,
    depth: u32,
    only_first: bool,
) -> Option<Vec<Tok>> {
    let mut input = flux_syntax::parser::Input::default();
    for t in toks {
        input.push(t.kind, t.is_union_keyword());
        if t.joint {
            input.set_last_joint();
        }
    }
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < toks.len() {
        let Some(n) =
            flux_syntax::parser::fragment_len(&input, pos, Fragment::Expr).filter(|n| *n > 0)
        else {
            out.extend_from_slice(&toks[pos..]);
            break;
        };
        let written = &toks[pos..pos + n];
        // A forwarded `expr` fragment keeps the invisible parentheses
        // that hold it together; a macro call inside is expanded anyway.
        let mut arg = written;
        while let [open, inner @ .., close] = arg
            && !inner.is_empty()
            && open.kind == T!['(']
            && close.kind == T![')']
            && open.origin.range.is_empty()
            && close.origin.range.is_empty()
            && inner.iter().try_fold(0u32, |depth, t| match t.kind {
                T!['('] => Some(depth + 1),
                T![')'] => depth.checked_sub(1),
                _ => Some(depth),
            }) == Some(0)
        {
            arg = inner;
        }
        let mut starts = Vec::new();
        let text = tt::write_text(arg, &mut starts);
        let origins: Vec<_> = starts
            .into_iter()
            .zip(arg.iter().map(|t| t.origin))
            .collect();
        let kinds: Vec<_> = origins
            .iter()
            .zip(arg)
            .map(|(&(start, _), token)| (start, token.kind))
            .collect();
        let editions: Vec<_> = origins
            .iter()
            .zip(arg)
            .map(|(&(start, _), token)| (start, token.edition))
            .collect();
        let parse = flux_syntax::parse_expansion_with_token_kinds(
            text.into(),
            &origins,
            &kinds,
            arg[0].range(),
            Fragment::Expr,
            site.items.edition(site.module),
        )
        .with_token_editions(&editions)
        .with_cfg_options(site.items.crates[site.krate.0 as usize].cfg.clone());
        let node = parse.tree().root().children().next()?;
        if node.kind() == SyntaxKind::MACRO_EXPR {
            let path = node.children().find_map(ast::Path::cast)?;
            let def = match resolve::<()>(site.items, path, &[], site.scope, site.module) {
                Ok(Resolved::Def(id)) => &site.items.macros[id],
                Ok(Resolved::Local(_)) => unreachable!(),
                Err(error) => {
                    diagnostics.push(error);
                    return None;
                }
            };
            let expansion = expand_at_depth(def, node, site, Fragment::Expr, diagnostics, depth)?;
            let expanded = expansion.parse.tree().root().children().next()?;
            let mut tokens = Vec::new();
            tokens_of(expanded, &mut tokens);
            // A declarative macro can itself produce a macro invocation.
            if expanded.kind() == SyntaxKind::MACRO_EXPR {
                if depth as usize >= site.items.crates[site.krate.0 as usize].recursion_limit {
                    diagnostics.push(Diagnostic::new(
                        node.text_range(),
                        "recursion limit reached during eager macro expansion",
                    ));
                    return None;
                }
                tokens = eager_arguments(&tokens, site, diagnostics, depth + 1, false)?;
            }
            out.extend(tokens);
        } else {
            out.extend_from_slice(written);
        }
        pos += n;
        if only_first {
            out.extend_from_slice(&toks[pos..]);
            return Some(out);
        }
        if pos < toks.len() {
            if toks[pos].kind != T![,] {
                out.extend_from_slice(&toks[pos..]);
                break;
            }
            out.push(toks[pos]);
            pos += 1;
        }
    }
    Some(out)
}

/// Where a macro call is: the range of the node, or for one a macro's
/// definition wrote, from its first token to its last, as rustc points
/// at `m!($x)` there whatever `$x` is.
pub(crate) fn call_range(call: SyntaxNode<'_>) -> TextRange {
    let tree = call.tree();
    let mut tokens = call.tokens().filter(|t| !t.kind().is_trivia());
    let first = tokens.next();
    let last = tokens.last().or(first);
    let (Some(first), Some(last)) = (first, last) else {
        return call.text_range();
    };
    match (
        tree.token_origin(first.index()),
        tree.token_origin(last.index()),
    ) {
        (Some(a), Some(b))
            if a.group == b.group
                && a.group != TokenOrigin::SYNTHETIC
                && a.range.start() <= b.range.end() =>
        {
            TextRange::new(a.range.start(), b.range.end())
        }
        _ => call.text_range(),
    }
}

/// A token as rustc's messages name it: `` `x` ``, `` keyword `fn` ``.
pub(crate) fn describe_kind(kind: SyntaxKind, text: &str) -> String {
    if kind == T![_] {
        format!("reserved identifier `{text}`")
    } else if kind.is_reserved_keyword() || matches!(kind, SyntaxKind::GEN_KW) {
        format!("reserved keyword `{text}`")
    } else if kind.is_keyword() {
        format!("keyword `{text}`")
    } else {
        format!("`{text}`")
    }
}

fn describe_token(kind: SyntaxKind, text: Symbol, toks: &[Tok], g: tt::Glued) -> String {
    if toks[g.raw as usize].is_empty_vis() {
        return "`vis` metavariable".to_owned();
    }
    if g.len > 1 {
        return tt::describe(toks, g);
    }
    describe_kind(kind, text.as_str())
}

pub(crate) fn expand_proc(
    host: &crate::ProcMacro,
    tokens: &[Tok],
    attributes: Option<&[Tok]>,
    site: &CallSite<'_>,
    call: TextRange,
    contexts: &mut Vec<(u32, u32)>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<Tok>> {
    let source = site.items.files.get(&site.real_file)?.clone();
    let to_host = |tokens: &[Tok]| {
        tokens
            .iter()
            .map(|t| crate::ProcMacroToken {
                edition: t.edition,
                kind: t.kind,
                text: t.text.to_string(),
                joint: t.joint,
                origin: t.origin,
            })
            .collect()
    };
    let mixed = fresh_ctxt();
    contexts.push((mixed, 0));
    let environment = site.items.crates[site.krate.0 as usize]
        .environment
        .as_deref()
        .cloned()
        .unwrap_or_default();
    let current_dir = match site.items.crates[site.krate.0 as usize]
        .compilation_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)
    {
        Ok(directory) => directory,
        Err(error) => {
            diagnostics.push(Diagnostic::new(
                call,
                format!("cannot determine proc-macro working directory: {error}"),
            ));
            return None;
        }
    };
    let result = host.expander.expand(crate::ProcMacroInput {
        invocation_crate: site.krate.0,
        tokens: to_host(tokens),
        attributes: attributes.map(to_host),
        range_base: source.range_base,
        source,
        call,
        mixed_context: mixed,
        environment,
        current_dir,
    });
    match result {
        Ok(tokens) => {
            site.items.crates[site.krate.0 as usize].record_macro_output(&tokens);
            Some(
                tokens
                    .into_iter()
                    .map(|t| Tok {
                        edition: t.edition,
                        kind: t.kind,
                        text: Symbol::intern(&t.text),
                        joint: t.joint,
                        origin: t.origin,
                    })
                    .collect(),
            )
        }
        Err(message) => {
            diagnostics.push(Diagnostic::new(
                call,
                format!("proc macro `{}`: {message}", host.name),
            ));
            None
        }
    }
}

pub(crate) fn parse_proc_output(
    tokens: &[Tok],
    site: &CallSite<'_>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Parse> {
    let mut starts = Vec::new();
    let text = tt::write_text(tokens, &mut starts);
    let origins: Vec<_> = starts
        .iter()
        .zip(tokens)
        .map(|(&start, t)| (start, t.origin))
        .collect();
    let kinds: Vec<_> = starts
        .iter()
        .zip(tokens)
        .map(|(&start, t)| (start, t.kind))
        .collect();
    let editions: Vec<_> = starts
        .iter()
        .zip(tokens)
        .map(|(&start, t)| (start, t.edition))
        .collect();
    let parse = flux_syntax::parse_expansion_with_token_kinds(
        text.into(),
        &origins,
        &kinds,
        range,
        Fragment::Items,
        site.items.edition(site.module),
    )
    .with_token_editions(&editions)
    .with_cfg_options(site.items.crates[site.krate.0 as usize].cfg.clone());
    for error in parse.errors() {
        diagnostics.push(Diagnostic::new(error.range, error.message.clone()));
    }
    parse.errors().is_empty().then_some(parse)
}
