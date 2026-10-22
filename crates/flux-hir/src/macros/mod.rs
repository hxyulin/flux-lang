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
    depth: u32,
    /// An ancestor further up, for finding one at a given depth in
    /// logarithmic time (skew-binary jump pointers).
    jump: Option<MacroScopeId>,
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
    NEXT_CTXT.fetch_add(1, Ordering::Relaxed)
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
const CORE_STD_MACROS: &[&str] = &["print", "println", "eprint", "eprintln", "dbg"];

impl MacroDef {
    /// Reads `macro_rules! name { rules }` from `call`, a macro call
    /// whose path is `macro_rules`, in `krate` at `file`. `None` if it
    /// has no name, which the parser reported.
    pub(crate) fn from_rules(
        call: SyntaxNode<'_>,
        krate: CrateId,
        file: FileId,
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
            let (rules, errors) = mbe::parse_rules(inner.to_vec(), name_range);
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
fn strip_delimiters(toks: &[Tok]) -> &[Tok] {
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
    /// The file the call is written in, not an expansion's.
    pub real_file: FileId,
}

/// A call's expansion: its tree, and the hygiene contexts it made, each
/// with the context its tokens had in the definition.
pub(crate) struct Expansion {
    pub parse: Parse,
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
    let core_crate = items
        .crates
        .iter()
        .position(|c| c.is_core)
        .map(|i| CrateId(i as u32));
    let exported = |k: CrateId| items.exported_macros.get(&(k, name)).copied();
    // A macro by path, through the def map: what modules export or import.
    let by_path = || {
        // Collecting items, the crate's names are not resolved yet.
        if items.def_map.scopes.len() < items.modules.len() {
            return None;
        }
        let lowered = crate::item_tree::lower_path(path, &mut Vec::new())?;
        match items.resolve(module, &lowered, crate::def_map::Ns::Macros) {
            Ok(crate::def_map::Resolved::Def(crate::def_map::Def::Macro(id))) => Some(id),
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
    if segments.len() > 1
        && !std_macro
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
            if let Some(&id) = items.macro_prelude.get(&name) {
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
        (_, [first, _]) if *first == sym::std => Err(Diagnostic::new(
            range,
            format!("the `{first}` crate is not supported yet; flux has only `core` and `alloc`"),
        )),
        _ => Err(not_found()),
    }
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
    let call_range = call_range(call);
    let mut toks = Vec::new();
    let tt = call
        .children()
        .find(|c| c.kind() == SyntaxKind::TOKEN_TREE)?;
    tokens_of(tt, &mut toks);
    let inner = strip_delimiters(&toks);
    // Where the input ends: its closing delimiter, or the whole call if
    // it is empty.
    let eof_range = match (inner.is_empty(), toks.last()) {
        (false, Some(close)) if tt::is_close(close.kind) => close.range(),
        _ => call_range,
    };
    let mut ctxts = Vec::new();
    let output = match &def.kind {
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
                let ctxt = *marks.borrow_mut().entry(t.origin.ctxt).or_insert_with(|| {
                    let new = fresh_ctxt();
                    ctxts_cell.borrow_mut().push((new, t.origin.ctxt));
                    new
                });
                let origin = if in_file && t.origin.group != TokenOrigin::SYNTHETIC {
                    TokenOrigin {
                        range: t.origin.range,
                        group: def.group,
                        ctxt,
                    }
                } else {
                    TokenOrigin {
                        range: call_range,
                        group: TokenOrigin::SYNTHETIC,
                        ctxt,
                    }
                };
                Tok { origin, ..*t }
            };
            let crate_tokens = |t: &Tok, out: &mut Vec<Tok>| {
                let t = mark(t);
                if def.krate == site.krate {
                    out.push(Tok {
                        kind: T![crate],
                        text: kw::Crate,
                        ..t
                    });
                } else {
                    let colon = Tok {
                        kind: T![:],
                        text: Symbol::intern(":"),
                        joint: true,
                        ..t
                    };
                    out.push(colon);
                    out.push(Tok {
                        joint: false,
                        ..colon
                    });
                    let name = site.items.crates[def.krate.0 as usize].name;
                    out.push(Tok {
                        kind: SyntaxKind::IDENT,
                        text: name,
                        ..t
                    });
                }
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
            let ctxt = fresh_ctxt();
            ctxts.push((ctxt, 0));
            // `core` itself names itself `crate`; `alloc` and the program
            // name it `::core`.
            let in_core = site.items.crates[site.krate.0 as usize].name == sym::core
                && site.items.crates[site.krate.0 as usize].is_core;
            let location = || {
                let file = site.items.files.get(&site.real_file);
                let Some(file) = file else {
                    return (1, 1, String::new());
                };
                let text = file.text();
                let offset = (call_range.start() as usize).min(text.len());
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
            let cx = BuiltinCx {
                name: def.name,
                toks: inner,
                call: call_range,
                ctxt,
                core: if in_core { "crate" } else { "::core" },
                location: &location,
                module_path: &module_path,
            };
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
    let parse = flux_syntax::parse_expansion(text.into(), &origins, call_range, fragment);
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
    Some(Expansion { parse, ctxts })
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
    if g.len > 1 {
        return tt::describe(toks, g);
    }
    describe_kind(kind, text.as_str())
}
