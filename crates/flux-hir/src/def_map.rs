//! What names mean: each module's scope, built from its items and its
//! `use` imports, and the resolution of paths against them.
//!
//! A scope has two namespaces, as in Rust: types (modules, structs) and
//! values (functions, and the constructors of tuple and unit structs). A
//! name not found in a module's scope is looked up in the prelude
//! (`core::prelude::rust_2024`), then among crates (`core`), then, for
//! types, among the primitive types, so items shadow them all.
//!
//! Imports are resolved together by iterating until nothing changes,
//! since one import can depend on another (`use a::b;` where `a` is
//! itself imported). Glob imports bring in every name visible to the
//! importing module, below explicit items and imports; a name two globs
//! disagree on is ambiguous, an error only where it is used.

use std::collections::HashMap;

use flux_intern::{Symbol, kw, sym};
use flux_syntax::TextRange;

use crate::item_tree::{
    AssocItem, ConstId, ImplTarget, ItemCounts, ItemTree, ModuleId, Path, PathAnchor, StructKind,
    TraitId, TypeRef, UseKind, Visibility, is_primitive,
};
use crate::{Diagnostic, FileId, FunctionId, StructId};

/// A namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ns {
    Types,
    Values,
}

/// An item a name can mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Def {
    Module(ModuleId),
    Struct(StructId),
    Trait(TraitId),
    Function(FunctionId),
    Const(ConstId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub def: Def,
    pub vis: Visibility,
    pub kind: BindingKind,
    /// The item or import that made it, for "defined multiple times".
    pub file: FileId,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// Defined here.
    Item,
    /// `use a::b;`
    Import,
    /// `use a::*;`
    Glob,
    /// Two globs bring different items under this name.
    AmbiguousGlob,
}

#[derive(Debug, Default, Clone)]
pub struct Scope {
    pub types: HashMap<Symbol, Binding>,
    pub values: HashMap<Symbol, Binding>,
}

impl Scope {
    fn ns(&self, ns: Ns) -> &HashMap<Symbol, Binding> {
        match ns {
            Ns::Types => &self.types,
            Ns::Values => &self.values,
        }
    }

    fn ns_mut(&mut self, ns: Ns) -> &mut HashMap<Symbol, Binding> {
        match ns {
            Ns::Types => &mut self.types,
            Ns::Values => &mut self.values,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DefMap {
    /// Each module's scope, by module index.
    pub scopes: Vec<Scope>,
    /// `core::prelude::rust_2024`, whose names every module sees.
    pub prelude: Option<ModuleId>,
    /// Names whose import failed, by module: uses of them are not
    /// reported again.
    pub failed_imports: std::collections::HashSet<(ModuleId, Symbol)>,
}

/// What a path means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Def(Def),
    /// A primitive type: `i32`.
    Primitive(Symbol),
    /// An associated function: `S::new`, `i32::abs`.
    Assoc(FunctionId),
    /// An associated constant: `S::X`, `i32::MAX`.
    AssocConst(ConstId),
    /// `Self`, in an `impl` block.
    SelfType,
    /// `S::f` where `S`'s inherent blocks have no `f`: the segments
    /// before this one name a type, and this one an item that type
    /// checking looks for among the traits it implements.
    TypeRelative(usize),
    /// `Tr::f`: function or constant `f` of trait `Tr`, named by the
    /// segment before it.
    TraitItem(TraitId, AssocItem),
}

/// Why a path did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// Its last segment names nothing; `module` is where it was looked
    /// for, `None` for a single name looked up in scope. The caller says
    /// what it was looking for: "cannot find function `f` in module `m`".
    NotFound {
        segment: usize,
        module: Option<ModuleId>,
    },
    /// It names something private here; it would mean `resolved`.
    Private {
        segment: usize,
        message: String,
        resolved: Resolved,
    },
    /// Anything else, with rustc's message.
    Failed { at: At, message: String },
    /// A name whose import failed, which was reported there.
    Reported,
}

/// The part of a path an error is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Segment(usize),
    /// The `n`th `crate`, `self` or `super` before the names.
    Anchor(usize),
    /// The whole path.
    Whole,
}

impl ResolveError {
    /// The range of the part of `path` this is about; `whole` if unknown.
    pub fn range(&self, path: &Path, whole: TextRange) -> TextRange {
        match self {
            ResolveError::NotFound { segment, .. } | ResolveError::Private { segment, .. } => {
                path.range(*segment, whole)
            }
            ResolveError::Failed {
                at: At::Segment(segment),
                ..
            } => path.range(*segment, whole),
            ResolveError::Failed {
                at: At::Anchor(n), ..
            } => path.anchor_ranges.get(*n).copied().unwrap_or(whole),
            ResolveError::Failed { at: At::Whole, .. } => whole,
            ResolveError::Reported => whole,
        }
    }

    /// The segment it is about, the first one for an anchor.
    pub fn segment(&self) -> usize {
        match self {
            ResolveError::NotFound { segment, .. }
            | ResolveError::Private { segment, .. }
            | ResolveError::Failed {
                at: At::Segment(segment),
                ..
            } => *segment,
            ResolveError::Failed { .. } | ResolveError::Reported => 0,
        }
    }
}

impl ItemTree {
    /// Resolves `path`, written in `from`, in namespace `ns`.
    pub fn resolve(&self, from: ModuleId, path: &Path, ns: Ns) -> Result<Resolved, ResolveError> {
        let mut private = None;
        let resolved = self.resolve_inner(from, path, ns, &mut private)?;
        match private {
            Some((segment, message)) => Err(ResolveError::Private {
                segment,
                message,
                resolved,
            }),
            None => Ok(resolved),
        }
    }

    /// Resolves `path`, recording the first private item on the way in
    /// `private` and carrying on, as rustc does.
    fn resolve_inner(
        &self,
        from: ModuleId,
        path: &Path,
        ns: Ns,
        private: &mut Option<(usize, String)>,
    ) -> Result<Resolved, ResolveError> {
        let fail = |at: At, message: String| ResolveError::Failed { at, message };
        let segments = &path.segments;
        let (mut module, start) = match path.anchor {
            PathAnchor::Crate => (self.module_of(self.modules[from].krate), 0),
            PathAnchor::SelfModule => (from, 0),
            PathAnchor::Super(n) => {
                let mut module = from;
                for k in 0..n {
                    match self.modules[module].parent {
                        Some(parent) => module = parent,
                        None => {
                            let message = format!(
                                "too many leading `super` keywords within `{}`",
                                self.module_path(from)
                            );
                            return Err(fail(At::Anchor(k as usize), message));
                        }
                    }
                }
                (module, 0)
            }
            PathAnchor::Global => {
                let Some(&first) = segments.first() else {
                    return Err(fail(At::Segment(0), "expected a name after `::`".into()));
                };
                match self.extern_crate(first, from) {
                    Some(root) => (root, 1),
                    None => {
                        let what = if segments.len() == 1 { "crate " } else { "" };
                        return Err(fail(
                            At::Segment(0),
                            format!("cannot find {what}`{first}` in the list of imported crates"),
                        ));
                    }
                }
            }
            PathAnchor::Plain => {
                let Some(&first) = segments.first() else {
                    return Err(fail(At::Segment(0), "expected a path".into()));
                };
                if let Some(binding) = self.def_map.scopes[from.index()]
                    .ns(if segments.len() == 1 { ns } else { Ns::Types })
                    .get(&first)
                    && binding.kind == BindingKind::AmbiguousGlob
                {
                    return Err(fail(At::Segment(0), format!("`{first}` is ambiguous")));
                }
                if self.def_map.failed_imports.contains(&(from, first))
                    && self.lookup(from, first, ns).is_none()
                {
                    return Err(ResolveError::Reported);
                }
                if segments.len() == 1 {
                    return self.lookup(from, first, ns).ok_or(ResolveError::NotFound {
                        segment: 0,
                        module: None,
                    });
                }
                match self.lookup(from, first, Ns::Types) {
                    Some(Resolved::Def(Def::Module(module))) => (module, 1),
                    Some(Resolved::Def(Def::Struct(strukt))) => {
                        return self.resolve_assoc(from, ImplTarget::Struct(strukt), path, 1, ns);
                    }
                    Some(Resolved::Primitive(name)) => {
                        return self.resolve_assoc(from, ImplTarget::Primitive(name), path, 1, ns);
                    }
                    Some(Resolved::Def(Def::Trait(t))) => {
                        return self.resolve_trait_item(t, path, 1, ns);
                    }
                    Some(Resolved::SelfType) => return Ok(Resolved::SelfType),
                    _ => {
                        let message = if first == sym::std || first == sym::alloc {
                            format!(
                                "the `{first}` crate is not supported yet; flux has only `core`"
                            )
                        } else {
                            format!("cannot find module or crate `{first}` in this scope")
                        };
                        return Err(fail(At::Segment(0), message));
                    }
                }
            }
        };
        if start == segments.len() {
            // `crate`, `self` or `super` alone: a module.
            return Ok(Resolved::Def(Def::Module(module)));
        }
        for i in start..segments.len() {
            let name = segments[i];
            let last = i + 1 == segments.len();
            let ns_here = if last { ns } else { Ns::Types };
            let Some(binding) = self.def_map.scopes[module.index()]
                .ns(ns_here)
                .get(&name)
                .copied()
            else {
                // flux's `core` is a small part of Rust's.
                if self.is_core(module) && !self.is_core(from) {
                    let shown: Vec<String> = segments[..=i].iter().map(|s| s.to_string()).collect();
                    let message = format!("`{}` is not supported yet", shown.join("::"));
                    return Err(fail(At::Segment(i), message));
                }
                if last {
                    return Err(ResolveError::NotFound {
                        segment: i,
                        module: Some(module),
                    });
                }
                let message = format!("cannot find `{name}` in {}", self.module_desc(module));
                return Err(fail(At::Segment(i), message));
            };
            if binding.kind == BindingKind::AmbiguousGlob {
                return Err(fail(At::Segment(i), format!("`{name}` is ambiguous")));
            }
            if private.is_none() && !self.is_visible(binding.vis, from) {
                *private = Some((i, self.private_message(binding, name, ns_here)));
            }
            match binding.def {
                Def::Module(m) if !last => module = m,
                Def::Struct(strukt) if !last => {
                    if i + 2 != segments.len() {
                        return Err(fail(At::Segment(0), "ambiguous associated type".into()));
                    }
                    return self.resolve_assoc(from, ImplTarget::Struct(strukt), path, i + 1, ns);
                }
                Def::Trait(t) if !last => return self.resolve_trait_item(t, path, i + 1, ns),
                def if last => return Ok(Resolved::Def(def)),
                _ => {
                    let message = format!("expected module, found `{name}`");
                    return Err(fail(At::Segment(i), message));
                }
            }
        }
        unreachable!("the loop returns at the last segment")
    }

    /// "function `f` is private", as rustc says it.
    fn private_message(&self, binding: Binding, name: Symbol, ns: Ns) -> String {
        let what = match binding.def {
            Def::Module(_) => "module",
            Def::Struct(s) if ns == Ns::Values && self.structs[s].kind == StructKind::Unit => {
                "unit struct"
            }
            Def::Struct(_) => "struct",
            Def::Trait(_) => "trait",
            Def::Function(_) => "function",
            Def::Const(_) => "constant",
        };
        let import = if binding.kind == BindingKind::Item {
            ""
        } else {
            " import"
        };
        format!("{what}{import} `{name}` is private")
    }

    /// `T::name`, where segment `index` is `name`.
    pub fn resolve_assoc(
        &self,
        from: ModuleId,
        target: ImplTarget,
        path: &Path,
        index: usize,
        ns: Ns,
    ) -> Result<Resolved, ResolveError> {
        let name = path.segments[index];
        if index + 1 != path.segments.len() {
            return Err(ResolveError::Failed {
                at: At::Segment(0),
                message: "ambiguous associated type".into(),
            });
        }
        // `S::Out` needs `<S as Tr>::Out`.
        if ns != Ns::Values {
            return Err(ResolveError::Failed {
                at: At::Whole,
                message: "ambiguous associated type".into(),
            });
        }
        let Some(item) = self.assoc_item(target, name) else {
            // Perhaps a trait's, which type checking finds.
            return Ok(Resolved::TypeRelative(index));
        };
        let (vis, what, resolved) = match item {
            AssocItem::Function(f) => {
                let data = &self.functions[f];
                let what = if data.has_self {
                    "method"
                } else {
                    "associated function"
                };
                (data.loc.vis, what, Resolved::Assoc(f))
            }
            AssocItem::Const(c) => (
                self.consts[c].loc.vis,
                "associated constant",
                Resolved::AssocConst(c),
            ),
        };
        if !self.is_visible(vis, from) {
            return Err(ResolveError::Private {
                segment: index,
                message: format!("{what} `{name}` is private"),
                resolved,
            });
        }
        Ok(resolved)
    }

    /// `Tr::name`, where segment `index` is `name`.
    fn resolve_trait_item(
        &self,
        t: TraitId,
        path: &Path,
        index: usize,
        ns: Ns,
    ) -> Result<Resolved, ResolveError> {
        let name = path.segments[index];
        if index + 1 != path.segments.len() || ns != Ns::Values {
            return Err(ResolveError::Failed {
                at: At::Whole,
                message: "ambiguous associated type".into(),
            });
        }
        let data = &self.traits[t];
        match data.value(self, name) {
            Some(item) => Ok(Resolved::TraitItem(t, item)),
            None => Err(ResolveError::Failed {
                at: At::Segment(index),
                message: format!(
                    "cannot find method or associated constant `{name}` in trait `{}`",
                    data.name
                ),
            }),
        }
    }

    /// A single name, in scope in `module`.
    pub fn lookup(&self, module: ModuleId, name: Symbol, ns: Ns) -> Option<Resolved> {
        if name == kw::SelfUpper && ns == Ns::Types {
            return Some(Resolved::SelfType);
        }
        let scope = &self.def_map.scopes[module.index()];
        if let Some(binding) = scope.ns(ns).get(&name)
            && binding.kind != BindingKind::AmbiguousGlob
        {
            return Some(Resolved::Def(binding.def));
        }
        if let Some(prelude) = self.def_map.prelude
            && let Some(binding) = self.def_map.scopes[prelude.index()].ns(ns).get(&name)
            && binding.vis == Visibility::Public
        {
            return Some(Resolved::Def(binding.def));
        }
        if ns == Ns::Types {
            if let Some(root) = self.extern_crate(name, module) {
                return Some(Resolved::Def(Def::Module(root)));
            }
            if is_primitive(name) {
                return Some(Resolved::Primitive(name));
            }
        }
        None
    }

    /// The root of the crate called `name`, as seen from `module`: `core`.
    fn extern_crate(&self, name: Symbol, module: ModuleId) -> Option<ModuleId> {
        let own = self.modules[module].krate;
        self.crates
            .iter()
            .enumerate()
            .find(|(i, c)| c.name == name && c.is_core && *i as u32 != own.0)
            .map(|(_, c)| c.root)
    }

    /// `crate::a::b`, as rustc names modules.
    pub fn module_path(&self, module: ModuleId) -> String {
        let m = &self.modules[module];
        match m.parent {
            Some(parent) => format!("{}::{}", self.module_path(parent), m.name),
            None if self.crates[m.krate.0 as usize].is_core && m.krate != self.main_crate() => {
                "core".to_owned()
            }
            None => "crate".to_owned(),
        }
    }

    /// "the crate root", "crate `core`" or "module `m`", for messages
    /// about names not found there.
    pub fn in_module(&self, module: ModuleId) -> String {
        let m = &self.modules[module];
        match m.parent {
            None if m.krate == self.main_crate() => "the crate root".to_owned(),
            None => format!("crate `{}`", m.name),
            Some(_) => format!("module `{}`", m.name),
        }
    }

    /// "the crate root", "`core`" or "`m`", for messages.
    pub fn module_desc(&self, module: ModuleId) -> String {
        let m = &self.modules[module];
        match m.parent {
            None if m.krate == self.main_crate() => "the crate root".to_owned(),
            _ => format!(
                "`{}`",
                if m.parent.is_none() {
                    sym::core
                } else {
                    m.name
                }
            ),
        }
    }

    /// The visibility of `def` itself.
    fn def_vis(&self, def: Def) -> Visibility {
        match def {
            Def::Module(m) => self.modules[m].vis,
            Def::Struct(s) => self.structs[s].loc.vis,
            Def::Trait(t) => self.traits[t].loc.vis,
            Def::Function(f) => self.functions[f].loc.vis,
            Def::Const(c) => self.consts[c].loc.vis,
        }
    }
}

/// Builds the scopes of the modules from `from` on and resolves the
/// imports and `impl` blocks from there; those before are resolved
/// already, in crates that cannot see the new one.
pub(crate) fn resolve(items: &mut ItemTree, from: ItemCounts) {
    items
        .def_map
        .scopes
        .resize(items.modules.len(), Scope::default());
    define_items(items, from);
    items.def_map.prelude = find_prelude(items);
    resolve_imports(items, from.uses as usize);
    resolve_impls(items, from.impls as usize);
    // An import into both namespaces reports its problems once.
    let mut seen = std::collections::HashSet::new();
    items
        .diagnostics
        .retain(|d| seen.insert((d.file, d.range, d.message.clone())));
}

fn define(items: &mut ItemTree, module: ModuleId, name: Symbol, ns: Ns, binding: Binding) -> bool {
    let scope = items.def_map.scopes[module.index()].ns_mut(ns);
    match scope.get(&name) {
        Some(existing)
            if existing.kind == BindingKind::Item || existing.kind == BindingKind::Import =>
        {
            false
        }
        _ => {
            scope.insert(name, binding);
            true
        }
    }
}

/// Puts each item from `from` on in its module's scope, reporting names
/// defined twice.
fn define_items(items: &mut ItemTree, from: ItemCounts) {
    // Module, name, namespaces and binding of each item.
    let mut defined: Vec<(ModuleId, Symbol, &[Ns], Binding)> = Vec::new();
    let item = |def, vis, file, range| Binding {
        def,
        vis,
        kind: BindingKind::Item,
        file,
        range,
    };
    for (id, m) in items.modules.iter().skip(from.modules as usize) {
        if let Some(parent) = m.parent {
            let file = items.modules[parent].file;
            let file = if m.def_range.is_empty() { m.file } else { file };
            let binding = item(Def::Module(id), m.vis, file, m.def_range);
            defined.push((parent, m.name, &[Ns::Types], binding));
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        let ns: &[Ns] = if s.kind == StructKind::Record {
            &[Ns::Types]
        } else {
            &[Ns::Types, Ns::Values]
        };
        let binding = item(Def::Struct(id), s.loc.vis, s.loc.file, s.def_range);
        defined.push((s.loc.module, s.name, ns, binding));
    }
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let binding = item(Def::Trait(id), t.loc.vis, t.loc.file, t.def_range);
        defined.push((t.loc.module, t.name, &[Ns::Types], binding));
    }
    for (id, c) in items.consts.iter().skip(from.consts as usize) {
        if let (None, Some(name)) = (c.owner, c.name) {
            let binding = item(Def::Const(id), c.loc.vis, c.loc.file, c.def_range);
            defined.push((c.loc.module, name, &[Ns::Values], binding));
        }
    }
    for (id, f) in items.functions.iter().skip(from.functions as usize) {
        if f.owner.is_none() {
            let binding = item(Def::Function(id), f.loc.vis, f.loc.file, f.def_range);
            defined.push((f.loc.module, f.name, &[Ns::Values], binding));
        }
    }
    defined.sort_by_key(|(.., b)| (b.file, b.range.start()));
    for (module, name, namespaces, binding) in defined {
        let mut ok = true;
        for &ns in namespaces {
            ok &= define(items, module, name, ns, binding);
        }
        if !ok {
            let message = format!("the name `{name}` is defined multiple times");
            error(items, binding.file, binding.range, message);
        }
    }
}

fn find_prelude(items: &ItemTree) -> Option<ModuleId> {
    let core = items.crates.iter().find(|c| c.is_core)?.root;
    let child = |module: ModuleId, name: Symbol| match items.def_map.scopes[module.index()]
        .types
        .get(&name)?
        .def
    {
        Def::Module(m) => Some(m),
        _ => None,
    };
    child(
        child(core, Symbol::intern("prelude"))?,
        Symbol::intern("rust_2024"),
    )
}

/// Resolves every `use` from `from` on, repeating until no import makes
/// progress.
fn resolve_imports(items: &mut ItemTree, from: usize) {
    let mut pending: Vec<usize> = (from..items.uses.len())
        .filter(|&i| items.uses[i].kind != UseKind::Glob)
        .collect();
    let globs: Vec<usize> = (from..items.uses.len())
        .filter(|&i| items.uses[i].kind == UseKind::Glob)
        .collect();
    loop {
        let mut progress = false;
        let mut still = Vec::new();
        for &i in &pending {
            let u = &items.uses[i];
            let module_only = matches!(u.kind, UseKind::Module(_));
            let mut found = Vec::new();
            let mut private = None;
            for ns in [Ns::Types, Ns::Values] {
                let def = match items.resolve(u.module, &u.path, ns) {
                    Ok(Resolved::Def(def)) => def,
                    Err(ResolveError::Private {
                        resolved: Resolved::Def(def),
                        segment,
                        message,
                    }) => {
                        private.get_or_insert((segment, message));
                        def
                    }
                    _ => continue,
                };
                if !module_only || matches!(def, Def::Module(_)) {
                    found.push((ns, def));
                }
            }
            if found.is_empty() {
                still.push(i);
                continue;
            }
            progress = true;
            if let Some((segment, message)) = private {
                let range = u.path.range(segment, u.range);
                error(items, u.file, range, message);
            }
            for (ns, def) in found {
                bind_import(items, i, ns, def);
            }
        }
        pending = still;
        for &g in &globs {
            progress |= apply_glob(items, g);
        }
        if !progress {
            break;
        }
    }
    for i in pending {
        report_unresolved(items, i);
        let u = &items.uses[i];
        if let UseKind::Named(name) | UseKind::Module(name) = u.kind {
            items.def_map.failed_imports.insert((u.module, name));
        }
    }
    for &g in &globs {
        let u = &items.uses[g];
        match items.resolve(u.module, &u.path, Ns::Types) {
            Ok(Resolved::Def(Def::Module(_))) => {}
            Err(ResolveError::Private {
                segment, message, ..
            }) => {
                let range = u.path.range(segment, u.range);
                error(items, u.file, range, message);
            }
            _ => report_unresolved(items, g),
        }
    }
}

fn error(items: &mut ItemTree, file: FileId, range: TextRange, message: impl Into<String>) {
    let mut d = Diagnostic::new(range, message);
    d.file = file;
    items.diagnostics.push(d);
}

/// Puts what import `index` names in its module's scope.
fn bind_import(items: &mut ItemTree, index: usize, ns: Ns, def: Def) {
    let u = &items.uses[index];
    let (UseKind::Named(name) | UseKind::Module(name)) = u.kind else {
        unreachable!("globs are applied elsewhere")
    };
    let (module, file, range) = (u.module, u.file, u.range);
    // A re-export may not make an item more visible than it is.
    let item_vis = items.def_vis(def);
    let wider = match (u.vis, item_vis) {
        (_, Visibility::Public) => false,
        (Visibility::Public, Visibility::Crate(_)) => true,
        (Visibility::Public | Visibility::Crate(_), Visibility::Module(_)) => true,
        _ => false,
    };
    let vis = if wider {
        let original = u.path.segments.last().copied().unwrap_or(name);
        let message = match item_vis {
            Visibility::Crate(_) => format!(
                "`{original}` is only public within the crate, and cannot be re-exported outside"
            ),
            _ => format!("`{original}` is private, and cannot be re-exported"),
        };
        error(items, file, range, message);
        item_vis
    } else {
        u.vis
    };
    let scope = items.def_map.scopes[module.index()].ns_mut(ns);
    match scope.get(&name).copied() {
        Some(existing) if matches!(existing.kind, BindingKind::Item | BindingKind::Import) => {
            // rustc reports whichever comes later.
            let (file, range) = if (existing.file, existing.range.start()) > (file, range.start()) {
                (existing.file, existing.range)
            } else {
                (file, range)
            };
            error(
                items,
                file,
                range,
                format!("the name `{name}` is defined multiple times"),
            );
        }
        _ => {
            scope.insert(
                name,
                Binding {
                    def,
                    vis,
                    kind: BindingKind::Import,
                    file,
                    range,
                },
            );
        }
    }
}

/// Reports an import that never resolved, as rustc does: the part of its
/// path that failed.
fn report_unresolved(items: &mut ItemTree, index: usize) {
    let u = &items.uses[index];
    let (file, range, path) = (u.file, u.range, u.path.clone());
    let shown = |k: usize| {
        let prefix = Path {
            segments: path.segments[..(k + 1).min(path.segments.len())].into(),
            ..path.clone()
        };
        format!("unresolved import `{prefix}`")
    };
    // A struct used as a module: `use m::S::new;`.
    for k in 0..path.segments.len().saturating_sub(1) {
        let prefix = Path {
            segments: path.segments[..=k].into(),
            ..path.clone()
        };
        if let Ok(Resolved::Def(Def::Struct(_))) = items.resolve(u.module, &prefix, Ns::Types) {
            let message = shown(k);
            return error(items, file, path.range(k, range), message);
        }
    }
    let (message, at) = match items.resolve(u.module, &path, Ns::Types) {
        Err(e @ ResolveError::Failed { .. }) => {
            let ResolveError::Failed { message, .. } = &e else {
                unreachable!()
            };
            let raw = [
                "super",
                "not supported",
                "ambiguous",
                "list of imported crates",
            ]
            .iter()
            .any(|w| message.contains(w));
            if raw {
                (message.clone(), e.range(&path, range))
            } else {
                (shown(e.segment()), e.range(&path, range))
            }
        }
        _ => (format!("unresolved import `{path}`"), range),
    };
    error(items, file, at, message);
}

impl ItemTree {
    /// The trait a bound names, written in `module`, and an error about
    /// it, with where it is, unless it was reported already: a private
    /// trait is still the trait.
    pub fn resolve_trait(
        &self,
        module: ModuleId,
        bound: &crate::Bound,
    ) -> (Option<TraitId>, Option<(TextRange, String)>) {
        let path = &bound.path;
        match self.resolve(module, path, Ns::Types) {
            Err(e @ ResolveError::Private { .. }) => {
                let range = e.range(path, bound.range);
                let ResolveError::Private {
                    resolved, message, ..
                } = e
                else {
                    unreachable!()
                };
                let t = match resolved {
                    Resolved::Def(Def::Trait(t)) => Some(t),
                    _ => None,
                };
                (t, Some((range, message)))
            }
            resolved => match self.trait_of(path, bound.range, resolved) {
                Ok(t) => (Some(t), None),
                Err(e) => (None, e),
            },
        }
    }

    fn trait_of(
        &self,
        path: &Path,
        range: TextRange,
        resolved: Result<Resolved, ResolveError>,
    ) -> Result<TraitId, Option<(TextRange, String)>> {
        let last = path.segments.len().saturating_sub(1);
        let at = path.range(last, range);
        let name = path.segments.get(last).copied().unwrap_or(kw::Empty);
        match resolved {
            Ok(Resolved::Def(Def::Trait(t))) => Ok(t),
            Ok(Resolved::Def(def)) => {
                let what = match def {
                    Def::Module(_) => "module",
                    Def::Struct(_) => "struct",
                    Def::Function(_) => "function",
                    Def::Const(_) => "constant",
                    Def::Trait(_) => unreachable!(),
                };
                Err(Some((at, format!("expected trait, found {what} `{name}`"))))
            }
            Ok(Resolved::Primitive(p)) => Err(Some((
                at,
                format!("expected trait, found builtin type `{p}`"),
            ))),
            Ok(Resolved::SelfType) => {
                Err(Some((at, "expected trait, found self type `Self`".into())))
            }
            Ok(_) => Err(Some((at, format!("expected trait, found `{path}`")))),
            Err(ResolveError::NotFound { module: None, .. }) => Err(Some((
                at,
                format!("cannot find trait `{name}` in this scope"),
            ))),
            Err(ResolveError::NotFound {
                module: Some(m), ..
            }) => Err(Some((
                at,
                format!("cannot find trait `{name}` in {}", self.in_module(m)),
            ))),
            Err(e @ ResolveError::Failed { .. }) => {
                let range = e.range(path, range);
                let ResolveError::Failed { message, .. } = e else {
                    unreachable!()
                };
                Err(Some((range, message)))
            }
            Err(ResolveError::Private { .. } | ResolveError::Reported) => Err(None),
        }
    }
}

/// An associated item's name, `impl` block, position, file and range.
type ItemInfo = (
    Option<Symbol>,
    Option<crate::item_tree::Owner>,
    (FileId, u32),
    FileId,
    TextRange,
);

fn item_info(items: &ItemTree, item: AssocItem) -> ItemInfo {
    match item {
        AssocItem::Function(f) => {
            let f = &items.functions[f];
            let at = (f.loc.file, f.def_range.start());
            (Some(f.name), f.owner, at, f.loc.file, f.def_range)
        }
        AssocItem::Const(c) => {
            let c = &items.consts[c];
            let at = (c.loc.file, c.def_range.start());
            (c.name, c.owner, at, c.loc.file, c.def_range)
        }
    }
}

/// Copies the names a glob import brings in; whether any were new.
fn apply_glob(items: &mut ItemTree, index: usize) -> bool {
    let u = &items.uses[index];
    let (module, vis, u_file, u_range) = (u.module, u.vis, u.file, u.range);
    let Ok(Resolved::Def(Def::Module(source))) = items.resolve(module, &u.path, Ns::Types) else {
        return false;
    };
    let mut changed = false;
    for ns in [Ns::Types, Ns::Values] {
        let names: Vec<(Symbol, Binding)> = items.def_map.scopes[source.index()]
            .ns(ns)
            .iter()
            .filter(|(_, b)| {
                b.kind != BindingKind::AmbiguousGlob && items.is_visible(b.vis, module)
            })
            .map(|(n, b)| (*n, *b))
            .collect();
        for (name, binding) in names {
            let scope = items.def_map.scopes[module.index()].ns_mut(ns);
            match scope.get(&name) {
                None => {
                    scope.insert(
                        name,
                        Binding {
                            def: binding.def,
                            vis,
                            kind: BindingKind::Glob,
                            file: u_file,
                            range: u_range,
                        },
                    );
                    changed = true;
                }
                Some(existing)
                    if existing.kind == BindingKind::Glob && existing.def != binding.def =>
                {
                    scope.insert(
                        name,
                        Binding {
                            kind: BindingKind::AmbiguousGlob,
                            ..*existing
                        },
                    );
                    changed = true;
                }
                Some(_) => {}
            }
        }
    }
    changed
}

/// Finds the type of each `impl` block from `from` on, and checks that
/// the associated functions of their types have distinct names.
fn resolve_impls(items: &mut ItemTree, from: usize) {
    let ids: Vec<_> = items.impls.iter().skip(from).map(|(id, _)| id).collect();
    for id in ids {
        let imp = &items.impls[id];
        let (module, file) = (imp.module, imp.file);
        let error = |items: &mut ItemTree, range, message: String| {
            let mut d = Diagnostic::new(range, message);
            d.file = file;
            items.diagnostics.push(d);
        };
        if let Some(bound) = imp.trait_ref.clone() {
            // The self type of a trait's block is resolved with
            // signatures; it may be any type.
            let (t, e) = items.resolve_trait(module, &bound);
            if let Some((range, message)) = e {
                error(items, range, message);
            }
            if let Some(t) = t {
                items.impls[id].trait_ = Some(t);
                items.trait_impls.entry(t).or_default().push(id);
            }
            continue;
        }
        let target = match &imp.self_ty {
            TypeRef::Path(path)
                if path
                    .as_name()
                    .is_some_and(|name| imp.generics.params.iter().any(|p| p.name == name)) =>
            {
                let range = imp.def_range;
                error(
                    items,
                    range,
                    "no nominal type found for inherent implementation".into(),
                );
                None
            }
            TypeRef::Path(path) => match items.resolve(module, path, Ns::Types) {
                Ok(Resolved::Def(Def::Struct(strukt))) => Some(ImplTarget::Struct(strukt)),
                Err(ResolveError::Private {
                    resolved: Resolved::Def(Def::Struct(strukt)),
                    segment,
                    message,
                }) => {
                    let range = path.range(segment, items.impls[id].self_range);
                    error(items, range, message);
                    Some(ImplTarget::Struct(strukt))
                }
                Ok(Resolved::Primitive(name)) if items.is_core(module) => {
                    Some(ImplTarget::Primitive(name))
                }
                Ok(Resolved::Primitive(_)) => {
                    let range = items.impls[id].def_range;
                    error(
                        items,
                        range,
                        "cannot define inherent `impl` for primitive types".into(),
                    );
                    None
                }
                Ok(_) => {
                    let range = items.impls[id].self_range;
                    error(
                        items,
                        range,
                        "`impl` blocks for types other than structs are not supported yet".into(),
                    );
                    None
                }
                Err(e) => {
                    let range = e.range(path, items.impls[id].self_range);
                    let message = match e {
                        ResolveError::NotFound {
                            module: None,
                            segment,
                        } => {
                            format!(
                                "cannot find type `{}` in this scope",
                                path.segments[segment]
                            )
                        }
                        ResolveError::NotFound {
                            module: Some(m),
                            segment,
                        } => format!(
                            "cannot find type `{}` in {}",
                            path.segments[segment],
                            items.in_module(m)
                        ),
                        ResolveError::Failed { message, .. }
                        | ResolveError::Private { message, .. } => message,
                        // Reported where the import failed.
                        ResolveError::Reported => String::new(),
                    };
                    if !message.is_empty() {
                        error(items, range, message);
                    }
                    None
                }
            },
            TypeRef::Error => None,
            _ => {
                let range = items.impls[id].def_range;
                error(
                    items,
                    range,
                    "cannot define inherent `impl` for primitive types".into(),
                );
                None
            }
        };
        items.impls[id].target = target;
    }
    for imp in items.impls.values().skip(from) {
        let Some(target) = imp.target else {
            continue;
        };
        let functions = imp.functions.iter().map(|&f| {
            let name = items.functions[f].name;
            (name, AssocItem::Function(f))
        });
        let consts = imp.consts.iter().filter_map(|&c| {
            let name = items.consts[c].name?;
            Some((name, AssocItem::Const(c)))
        });
        for (name, item) in functions.chain(consts).collect::<Vec<_>>() {
            items.assoc.entry((target, name)).or_default().push(item);
        }
    }
    // Duplicate associated items, per type with new blocks: functions and
    // constants share a namespace.
    let mut by_target: HashMap<ImplTarget, Vec<AssocItem>> = items
        .impls
        .values()
        .skip(from)
        .filter_map(|imp| Some((imp.target?, Vec::new())))
        .collect();
    for imp in items.impls.values() {
        if let Some(target) = imp.target
            && let Some(entry) = by_target.get_mut(&target)
        {
            entry.extend(imp.functions.iter().map(|&f| AssocItem::Function(f)));
            entry.extend(imp.consts.iter().map(|&c| AssocItem::Const(c)));
        }
    }
    let mut targets: Vec<_> = by_target.into_iter().collect();
    targets.sort_by_key(|(_, assoc)| assoc.first().map(|&a| item_info(items, a).2));
    for (target, mut assoc) in targets {
        assoc.sort_by_key(|&a| item_info(items, a).2);
        let mut seen: Vec<AssocItem> = Vec::new();
        for item in assoc {
            let (name, owner, _, file, range) = item_info(items, item);
            let Some(name) = name else {
                continue;
            };
            let first = seen
                .iter()
                .copied()
                .find(|&s| item_info(items, s).0 == Some(name));
            let Some(first) = first else {
                seen.push(item);
                continue;
            };
            // rustc reports the later definition when both are in one
            // block, and the earlier one otherwise.
            let (_, first_owner, _, first_file, first_range) = item_info(items, first);
            // Blocks for different instances of a generic struct may
            // define the same name; type checking sees which overlap.
            if first_owner != owner
                && let ImplTarget::Struct(strukt) = target
                && !items.structs[strukt].generics.is_empty()
            {
                continue;
            }
            let (file, range) = if first_owner == owner {
                (file, range)
            } else {
                (first_file, first_range)
            };
            error(
                items,
                file,
                range,
                format!("duplicate definitions with name `{name}`"),
            );
        }
    }
}
