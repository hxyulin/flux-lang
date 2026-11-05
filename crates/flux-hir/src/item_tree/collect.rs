//! File and macro item collection, followed by derives and constructors.
mod proc_macros;
use super::{
    Abi, AnonConst, Arc, AstNode, Block, BodyExpansion, Bound, Const, ConstId, CrateData, CrateId,
    Defaults, Derive, Diagnostic, Discriminant, FIELD_TYPES, Field, FileId, FsPath, Function,
    FunctionId, GenericArgs, GenericParam, Generics, IN_BOUNDS, Impl, ImplId, ImplParams,
    ImplTraitSite, ImplType, ItemLoc, ItemTree, LifetimeRef, LoadedCrates, MacroDef, MacroId,
    MacroImport, MacroScopeId, Module, ModuleId, NodeId, Owner, Param, ParamKind, Path, PathAnchor,
    PathBuf, RECURSION_LIMIT, Relaxed, SourceDb, Struct, StructId, StructKind, Symbol, SyntaxKind,
    SyntaxNode, TextRange, Trait, TraitId, TraitType, TypeAlias, TypeOutlives, TypeRef, TypesFile,
    Use, UseKind, Variant, Visibility, WherePredicate, ast, attr_name, check_attr, check_attrs,
    check_inert_attrs, check_inner_attrs, collect_generics, collect_generics_with_drop,
    collect_opaques, const_expr_root, core_path, def_start, derive_attr, desugar_impl_params,
    discriminant_value, enum_repr, expand_types, function_abi, generic_in_discriminant, int_range,
    is_anon_const, is_item_kind, item_enabled, item_enabled_with_test_context, kw, lang_attr,
    lower_bound_type, lower_bounds, lower_lifetime, lower_path, lower_type, macros,
    non_exhaustive_attr, on_unimplemented_attr, param_projections, reject_impl_trait,
    reject_impl_trait_in_path, repr_attr, struct_repr, sym, target_feature_attr,
    target_feature_inline, unsupported, with_macro_types,
};

/// Associated definitions may place predicates before or after the RHS. Keep
/// both sets in the same family-local scope, including their source ranges.
fn associated_generics(t: ast::TypeAlias<'_>, diagnostics: &mut Vec<Diagnostic>) -> Generics {
    let mut clauses = t.where_clauses();
    let mut generics = collect_generics(
        t.generic_param_list(),
        clauses.next(),
        Defaults::No,
        diagnostics,
    );
    for clause in clauses {
        let more = collect_generics(None, Some(clause), Defaults::No, diagnostics);
        let mut predicates = generics.predicates.into_vec();
        predicates.extend(more.predicates);
        generics.predicates = predicates.into();
        let mut outlives = generics.outlives.into_vec();
        outlives.extend(more.outlives);
        generics.outlives = outlives.into();
        let mut type_outlives = generics.type_outlives.into_vec();
        type_outlives.extend(more.type_outlives);
        generics.type_outlives = type_outlives.into();
    }
    generics
}

impl ItemTree {
    pub(crate) fn add_source_crate(
        &mut self,
        db: &mut SourceDb,
        root: FileId,
        name: Symbol,
        is_core: bool,
        dependencies: Vec<(Symbol, CrateId)>,
    ) {
        let from = self.counts();
        let mut collector = Collector {
            db,
            items: self,
            derives: Vec::new(),
            ctors: Vec::new(),
            block_counts: rustc_hash::FxHashMap::default(),
            fn_ids: rustc_hash::FxHashMap::default(),
            impl_ids: rustc_hash::FxHashMap::default(),
            adt_ids: rustc_hash::FxHashMap::default(),
            macro_scope: None,
            expansion_depth: 0,
            expanding_types: false,
            macro_paths: Vec::new(),
            collected_imports: rustc_hash::FxHashSet::default(),
            pending_items: Vec::new(),
            pending_body_macros: Vec::new(),
            legacy_calls: Vec::new(),
            body_expansion_consts: Vec::new(),
        };
        collector.collect_crate(root, name, is_core, dependencies);
        collector.collect_pending_items(from);
        collector.collect_pending_body_macros(from);
        collector.expand_derives();
        collector.make_ctors();
        let macro_paths = std::mem::take(&mut collector.macro_paths);
        crate::def_map::resolve(collector.items, from);
        collector.collect_body_expansion_consts();
        collector.collect_legacy_consts();
        self.resolve_sized_dispatch(from);
        // Early predecessor lookup must agree with completed scopes. A local
        // item/import declared later may shadow a dependency's import name.
        for call in macro_paths {
            if !matches!(self.resolve(call.module, &call.path, crate::def_map::Ns::Macros),
                Ok(crate::def_map::Resolved::Def(crate::def_map::Def::Macro(id))) if id == call.expected)
            {
                self.diagnostics.push(Diagnostic {
                    file: call.file,
                    range: call.range,
                    message: format!(
                        "cannot find macro `{}` in this scope",
                        self.macros[call.expected].name
                    ),
                });
            }
        }
    }
}

/// Reads crates' files and collects their items.
struct MacroPathCheck {
    module: ModuleId,
    path: Path,
    expected: MacroId,
    file: FileId,
    range: TextRange,
}

/// A macro import can refer to a module collected later. Keep its original
/// textual scope and associated owner while waiting for the item namespace.
enum PendingItemKind {
    Macro(Option<AssocOwner>),
    Attribute,
}

/// Where a macro call among associated items puts the items it expands
/// to: an `impl` block (and whether it implements a trait) or a trait.
#[derive(Clone, Copy)]
enum AssocOwner {
    Impl(ImplId, bool),
    Trait(TraitId),
}
struct PendingItem {
    kind: PendingItemKind,
    ctx: Ctx,
    node: NodeId,
    scope: Option<MacroScopeId>,
    depth: u32,
    diagnostic: Option<Diagnostic>,
}

/// An expression macro may import an export from a module collected later.
/// Keep its original textual environment rather than rewalking the body.
struct PendingBodyMacro {
    ctx: Ctx,
    node: NodeId,
    function: Option<FunctionId>,
    scope: Option<MacroScopeId>,
    depth: u32,
}

/// Direct calls are resolved after imports, before anonymous constants are checked.
struct LegacyCall {
    ctx: Ctx,
    node: NodeId,
    function: FunctionId,
    macro_scope: Option<MacroScopeId>,
}

/// A retained expansion of a macro call in `function`'s body, whose
/// anonymous constants are collected once the function's are.
struct BodyExpansionConsts {
    /// The expansion's file and the module of the block it is in.
    ctx: Ctx,
    /// The call, by the file it is in.
    call: (FileId, NodeId),
    function: FunctionId,
}

struct Collector<'a> {
    legacy_calls: Vec<LegacyCall>,
    body_expansion_consts: Vec<BodyExpansionConsts>,
    pending_items: Vec<PendingItem>,
    pending_body_macros: Vec<PendingBodyMacro>,
    collected_imports: rustc_hash::FxHashSet<(FileId, NodeId)>,
    macro_paths: Vec<MacroPathCheck>,
    db: &'a mut SourceDb,
    items: &'a mut ItemTree,
    /// The `#[derive]`s of a crate's structs, made into `impl` blocks
    /// after its other items, as rustc expands them.
    derives: Vec<(Ctx, StructId, Derive, TextRange, NodeId)>,
    /// The crate's structs and enums, whose tuple constructors become
    /// functions after its other items.
    ctors: Vec<(Ctx, StructId, NodeId)>,
    /// How many blocks with items each function has had, for their
    /// modules' names.
    block_counts: rustc_hash::FxHashMap<Option<FunctionId>, u32>,
    /// The functions and `impl` blocks collected, by their nodes: what
    /// their anonymous constants are in.
    fn_ids: rustc_hash::FxHashMap<(FileId, NodeId), FunctionId>,
    impl_ids: rustc_hash::FxHashMap<(FileId, NodeId), ImplId>,
    adt_ids: rustc_hash::FxHashMap<(FileId, NodeId), StructId>,
    /// The `macro_rules!` macros in textual scope where collecting is.
    macro_scope: Option<MacroScopeId>,
    /// How many macro expansions the items being collected are in.
    expansion_depth: u32,
    /// The macro calls in type position of the item being collected
    /// have been expanded.
    expanding_types: bool,
}

/// What the anonymous constants of an item see: the module, the `impl`
/// block (if `Self` may be used), the generic parameters (and whether
/// each is a const one), whether `Self` is generic, the function, and
/// the item's name, for rustc's names of them.
#[derive(Clone)]
struct AnonScope {
    self_adt: Option<StructId>,
    module: ModuleId,
    owner: Option<Owner>,
    generics: Vec<(Symbol, bool)>,
    generic_self: bool,
    function: Option<FunctionId>,
    item: Symbol,
}

/// Where an anonymous constant is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AnonUse {
    /// An array type's length.
    Len,
    /// A repeat expression's count.
    Repeat,
    /// A const generic argument.
    Arg,
}

/// What collecting is inside of.
#[derive(Clone, Copy)]
struct Ctx {
    module: ModuleId,
    file: FileId,
}

impl Collector<'_> {
    /// Gives the diagnostics added since `start` without a file `file`.
    fn tag(&mut self, start: usize, file: FileId) {
        for d in &mut self.items.diagnostics[start..] {
            if d.file == FileId::UNKNOWN {
                d.file = file;
            }
        }
    }

    fn collect_crate(
        &mut self,
        root: FileId,
        name: Symbol,
        is_core: bool,
        dependencies: Vec<(Symbol, CrateId)>,
    ) {
        let krate = CrateId(self.items.crates.len() as u32);
        let dir = self
            .db
            .file(root)
            .path
            .parent()
            .map_or_else(PathBuf::new, FsPath::to_path_buf);
        let module = self.items.modules.alloc(Module {
            name,
            parent: None,
            krate,
            file: root,
            dir,
            vis: Visibility::Public,
            def_range: TextRange::empty(0),
            block: None,
        });
        self.items.crates.push(CrateData {
            input_trace: crate::inputs::InputTrace::default(),
            compilation_dir: self.db.crate_directory(root),
            environment: self.db.crate_env(root),
            macro_use: Vec::new(),
            recursion_limit: RECURSION_LIMIT as usize,
            cfg: self.db.file(root).parse.tree().cfg_options().cloned(),
            edition: self.db.file(root).edition,
            name,
            root: module,
            is_core,
            externs: Vec::new(),
            extern_aliases: Vec::new(),
            loaded_externs: LoadedCrates::default(),
            dependencies,
            no_std: false,
        });
        if let Some(exports) = self.db.proc_macros(root) {
            self.items.insert_file(root, self.db.shared(root));
            self.items.crates[krate.0 as usize].no_std = true;
            for host in exports {
                let name = host.name;
                let id = self.items.macros.alloc(macros::MacroDef {
                    name,
                    kind: macros::MacroKind::ProcMacro(Arc::new(host.clone())),
                    krate,
                    file: root,
                    name_range: TextRange::empty(0),
                    group: 0,
                });
                self.items.exported_macros.insert((krate, name), id);
            }
        } else {
            self.collect_file(module, root);
        }
    }

    fn record_macro_path(&mut self, ctx: Ctx, path: ast::Path<'_>, expected: MacroId) {
        let def = &self.items.macros[expected];
        if def.krate == self.items.modules[ctx.module].krate
            || self.items.crates[def.krate.0 as usize].is_core
        {
            return;
        }
        if let Some(lowered) = lower_path(path, &mut Vec::new()) {
            self.macro_paths.push(MacroPathCheck {
                module: ctx.module,
                path: lowered,
                expected,
                file: self.real_file(ctx.file),
                range: path.syntax().text_range(),
            });
        }
    }

    /// The items of a whole file, the contents of `module`.
    fn collect_file(&mut self, module: ModuleId, file: FileId) {
        let start = self.items.diagnostics.len();
        let shared = self.db.shared(file);
        self.items.insert_file(file, Arc::clone(&shared));
        let tree = shared.parse.tree();
        let source = ast::SourceFile::cast(tree.root()).expect("the root is a SOURCE_FILE");
        let is_core = self.items.is_core(module);
        if !item_enabled(source.syntax(), &mut self.items.diagnostics) {
            self.tag(start, file);
            return;
        }
        let crate_root = self.items.modules[module].parent.is_none();
        let attrs = crate::cfg::Attrs::new(source.attrs());
        if crate_root {
            if let Some(limit) = attrs.iter().find_map(|attr| {
                (attr_name(attr) == Some("recursion_limit"))
                    .then(|| super::attributes::recursion_limit_attr(attr))
                    .flatten()
            }) {
                let own = self.items.modules[module].krate;
                self.items.crates[own.0 as usize].recursion_limit = limit;
            }
            let expected = self.items.crates[self.items.modules[module].krate.0 as usize].name;
            for attr in attrs.iter().filter(|a| attr_name(*a) == Some("crate_name")) {
                if let Some(ast::Expr::Literal(literal)) = attr.meta().and_then(|m| m.expr())
                    && let Some(token) = literal.value().filter(|t| t.kind() == SyntaxKind::STRING)
                    && let Ok(name) = crate::literal::unescape_str(token.text())
                    && name != expected.as_str()
                {
                    self.items.diagnostics.push(Diagnostic::new(
                        attr.syntax().text_range(),
                        "crate name does not match the supplied crate name",
                    ));
                }
            }
        }
        let no_std = crate_root && attrs.iter().any(|a| attr_name(a) == Some("no_std"));
        if no_std {
            let own = self.items.modules[module].krate;
            self.items.crates[own.0 as usize].no_std = true;
        }
        check_inner_attrs(
            source.attrs(),
            is_core,
            crate_root,
            &mut self.items.diagnostics,
        );
        let items: Vec<NodeId> = source.items().map(|i| i.syntax().id()).collect();
        self.collect_items(Ctx { module, file }, &items);
        self.tag(start, file);
    }

    /// Items by node, so that collecting them can read more files.
    fn collect_items(&mut self, ctx: Ctx, items: &[NodeId]) {
        let _file = TypesFile::enter(ctx.file);
        self.precollect_imports(ctx, items.iter().copied());
        for &node in items {
            let start = self.items.diagnostics.len();
            let shared = self.db.shared(ctx.file);
            if item_enabled(shared.parse.tree().node(node), &mut self.items.diagnostics) {
                let syntax = shared.parse.tree().node(node);
                let unresolved_attribute = self.db.has_proc_macros()
                    && crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast))
                        .iter()
                        .any(|attr| {
                            if self.is_derive_attr(ctx, attr) {
                                return false;
                            }
                            let Some(path) = attr
                                .meta()
                                .and_then(|meta| meta.path())
                                .filter(|path| path.qualifier().is_some())
                            else {
                                return false;
                            };
                            let mut errors = Vec::new();
                            check_attr(attr, &mut errors);
                            if errors.is_empty() {
                                return false;
                            }
                            let mut tokens = Vec::new();
                            macros::tt::tokens_of(path.syntax(), &mut tokens);
                            self.proc_path(ctx, &tokens).is_none()
                        });
                if unresolved_attribute {
                    self.pending_items.push(PendingItem {
                        kind: PendingItemKind::Attribute,
                        ctx,
                        node,
                        scope: self.macro_scope,
                        depth: self.expansion_depth,
                        diagnostic: None,
                    });
                    self.tag(start, ctx.file);
                    continue;
                }
                if self.collect_proc_item(ctx, node) {
                    self.tag(start, ctx.file);
                    continue;
                }
                self.collect_item_expanding_types(ctx, node);
                self.collect_anon_consts(ctx, node);
            }
            self.tag(start, ctx.file);
        }
    }

    /// External imports have item scope, so an invocation can precede its
    /// alias. Imports of current textual macros still follow their definitions.
    fn precollect_imports(&mut self, ctx: Ctx, nodes: impl Iterator<Item = NodeId>) {
        let shared = self.db.shared(ctx.file);
        for node in nodes {
            let syntax = shared.parse.tree().node(node);
            fn external_tree(
                items: &ItemTree,
                module: ModuleId,
                tree: ast::UseTree<'_>,
                prefix: bool,
            ) -> bool {
                let external = prefix
                    || tree
                        .path()
                        .and_then(|path| lower_path(path, &mut Vec::new()))
                        .is_some_and(|path| {
                            matches!(path.anchor, PathAnchor::Plain | PathAnchor::Global)
                                && (path.segments.len() > 1 || tree.use_tree_list().is_some())
                                && path
                                    .segments
                                    .first()
                                    .is_some_and(|&name| items.extern_crate(name, module).is_some())
                        });
                if let Some(list) = tree.use_tree_list() {
                    list.use_trees()
                        .all(|tree| external_tree(items, module, tree, external))
                } else {
                    external
                }
            }
            let external = syntax.kind() == SyntaxKind::EXTERN_CRATE
                || ast::Use::cast(syntax)
                    .and_then(|u| u.use_tree())
                    .is_some_and(|tree| external_tree(self.items, ctx.module, tree, false));
            // An attribute macro may remove or replace this import. Do not
            // expose it before expansion; unresolved custom attributes must
            // also wait until all ordinary imports have been collected.
            let attrs = crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast));
            let may_replace = self.db.has_proc_macros()
                && attrs.iter().any(|attr| {
                    let mut errors = Vec::new();
                    check_attr(attr, &mut errors);
                    if !errors.is_empty() {
                        return true;
                    }
                    let Some(path) = attr.meta().and_then(|m| m.path()) else {
                        return false;
                    };
                    let mut tokens = Vec::new();
                    macros::tt::tokens_of(path.syntax(), &mut tokens);
                    self.proc_path(ctx, &tokens).is_some()
                });
            if external && !may_replace && item_enabled(syntax, &mut Vec::new()) {
                let start = self.items.diagnostics.len();
                self.collect_item(ctx, node);
                self.tag(start, ctx.file);
            }
        }
    }

    /// [`Collector::collect_item`], with the macro calls in type position
    /// in it expanded first, unless an item around it did that.
    fn collect_item_expanding_types(&mut self, ctx: Ctx, node: NodeId) {
        if self.expanding_types {
            self.collect_item(ctx, node);
            return;
        }
        let shared = self.db.shared(ctx.file);
        let syntax = shared.parse.tree().node(node);
        if !syntax
            .descendants()
            .any(|d| d.kind() == SyntaxKind::MACRO_TYPE)
        {
            self.collect_item(ctx, node);
            return;
        }
        with_macro_types(|| {
            let mut diagnostics = Vec::new();
            expand_types(syntax, &mut diagnostics, &mut |call, diagnostics| {
                self.expand_type_call(ctx, call, diagnostics)
            });
            self.items.diagnostics.extend(diagnostics);
            self.expanding_types = true;
            self.collect_item(ctx, node);
            self.expanding_types = false;
        });
    }

    /// Expands `call`, a macro call in type position.
    fn expand_type_call(
        &mut self,
        ctx: Ctx,
        call: SyntaxNode<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<flux_syntax::Parse> {
        let path = call.children().find_map(ast::Path::cast)?;
        let def = match macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module) {
            Ok(macros::Resolved::Def(id)) => id,
            Ok(macros::Resolved::Local(_)) => unreachable!("no local macros here"),
            Err(d) => {
                diagnostics.push(d);
                return None;
            }
        };
        self.record_macro_path(ctx, path, def);
        let def = &self.items.macros[def];
        let site = macros::CallSite {
            read_text: crate::source::SourceDb::read_include_str,
            scope: self.macro_scope,
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: self.real_file(ctx.file),
        };
        let expansion = macros::expand(
            def,
            call,
            &site,
            flux_syntax::parser::Fragment::Type,
            diagnostics,
        )?;
        Some(expansion.parse)
    }

    fn collect_item(&mut self, ctx: Ctx, node: NodeId) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let item = ast::Item::cast(tree.node(node)).expect("items are ITEM nodes");
        match item {
            ast::Item::Fn(f) => {
                self.collect_fn(ctx, f, None, None);
            }
            ast::Item::Impl(imp) => {
                self.collect_impl(ctx, imp);
            }
            ast::Item::ExternBlock(block) => {
                self.collect_extern_block(ctx, block);
            }
            ast::Item::Union(u) => self.collect_union(ctx, u),
            ast::Item::Struct(s) => {
                self.collect_struct(ctx, s);
            }
            ast::Item::Module(m) => {
                self.collect_module(ctx, m);
            }
            ast::Item::Use(u) => {
                if self.collected_imports.insert((ctx.file, node)) {
                    self.collect_use(ctx, u);
                }
            }
            ast::Item::Const(c) => {
                self.collect_const(ctx, c, None);
            }
            ast::Item::Enum(e) => {
                self.collect_enum(ctx, e);
            }
            ast::Item::ExternCrate(e) => {
                if self.collected_imports.insert((ctx.file, node)) {
                    self.collect_extern_crate(ctx, e);
                }
            }
            ast::Item::MacroCall(mc) => {
                self.collect_macro_call(ctx, mc);
            }
            ast::Item::Static(st) => {
                self.collect_static(ctx, st, false);
            }
            ast::Item::Trait(t) => {
                self.collect_trait(ctx, t);
            }
            ast::Item::TypeAlias(t) => {
                self.collect_type_alias(ctx, t);
            }
        }
    }

    /// The file `file` is part of: itself, or the file of the call a
    /// macro expansion's items are from.
    fn real_file(&self, file: FileId) -> FileId {
        self.items
            .expansion_files
            .get(&file)
            .copied()
            .unwrap_or(file)
    }

    /// Defines the macro `macro_rules! name { .. }` at `node`, in textual
    /// scope from here on.
    fn define_macro(&mut self, ctx: Ctx, node: SyntaxNode<'_>, export: bool, builtin: bool) {
        let krate = self.items.modules[ctx.module].krate;
        let file = node
            .tokens()
            .find(|token| !token.kind().is_trivia())
            .and_then(|token| node.tree().token_origin(token.index()))
            .and_then(|origin| {
                self.items
                    .macros
                    .iter()
                    .find_map(|(_, def)| (def.group == origin.group).then_some(def.file))
            })
            .unwrap_or_else(|| self.real_file(ctx.file));
        let Some(def) = MacroDef::from_rules(
            node,
            krate,
            file,
            self.db.file(file).edition,
            builtin,
            &mut self.items.diagnostics,
        ) else {
            return;
        };
        let name = def.name;
        let id = self.items.macros.alloc(def);
        self.macro_scope = Some(self.items.push_macro_scope(self.macro_scope, name, id));
        if export {
            self.items.exported_macros.insert((krate, name), id);
            let krate_data = &self.items.crates[krate.0 as usize];
            if !matches!(
                self.items.macros[id].kind,
                macros::MacroKind::Builtin(macros::Builtin::Asm)
            ) && (krate_data.is_core
                || self.items.modules[krate_data.root].file == FileId::std_root())
            {
                self.items.macro_prelude.insert(name, id);
            }
        }
    }

    /// A macro call in item position: a `macro_rules!` definition, or a
    /// call whose expansion's items are collected as if written here.
    fn collect_macro_call(&mut self, ctx: Ctx, mc: ast::MacroCall<'_>) {
        let is_core = self.items.is_core(ctx.module);
        let (mut export, mut builtin) = (false, false);
        let attrs = crate::cfg::Attrs::new(mc.attrs());
        for attr in attrs.iter() {
            match attr_name(attr) {
                Some("macro_export") => export = true,
                Some("rustc_builtin_macro") if is_core => builtin = true,
                _ => check_attr(attr, &mut self.items.diagnostics),
            }
        }
        let node = mc.syntax();
        if macros::is_macro_rules(node) {
            self.define_macro(ctx, node, export, builtin);
            return;
        }
        let Some(path) = mc.path() else {
            return;
        };
        let resolved = macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module);
        let def = match resolved {
            Ok(macros::Resolved::Def(id)) => id,
            Ok(macros::Resolved::Local(_)) => unreachable!("no local macros here"),
            Err(d) => {
                self.pending_items.push(PendingItem {
                    kind: PendingItemKind::Macro(None),
                    ctx,
                    node: node.id(),
                    scope: self.macro_scope,
                    depth: self.expansion_depth,
                    diagnostic: Some(d),
                });
                return;
            }
        };
        self.collect_resolved_macro_call(ctx, mc, def);
    }

    fn collect_resolved_macro_call(&mut self, ctx: Ctx, mc: ast::MacroCall<'_>, def: MacroId) {
        let path = mc.path().expect("resolved macro path");
        let node = mc.syntax();
        self.record_macro_path(ctx, path, def);
        let def = self.items.macros[def].clone();
        if self.expansion_depth as usize >= self.items.macro_recursion_limit(ctx.module) {
            self.items.diagnostics.push(Diagnostic::new(
                node.text_range(),
                format!("recursion limit reached while expanding `{}!`", def.name),
            ));
            return;
        }
        let real = self.real_file(ctx.file);
        let site = macros::CallSite {
            read_text: crate::source::SourceDb::read_include_str,
            scope: self.macro_scope,
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: real,
        };
        let mut diagnostics = Vec::new();
        let expansion = macros::expand(
            &def,
            node,
            &site,
            flux_syntax::parser::Fragment::Items,
            &mut diagnostics,
        );
        self.items.diagnostics.extend(diagnostics);
        let Some(expansion) = expansion else {
            return;
        };
        let file = if let Some(path) = expansion.included_path {
            match self
                .db
                .add_inclusion(expansion.parse, path, self.db.file(ctx.file).edition)
            {
                Ok(file) => file,
                Err(message) => {
                    self.items
                        .diagnostics
                        .push(Diagnostic::new(node.text_range(), message));
                    return;
                }
            }
        } else {
            let file = self.db.add_expansion(expansion.parse, real);
            self.items.expansion_files.insert(file, real);
            file
        };
        self.items.insert_file(file, self.db.shared(file));
        let shared = self.db.shared(file);
        let items: Vec<NodeId> = shared
            .parse
            .tree()
            .root()
            .children()
            .filter(|c| is_item_kind(c.kind()))
            .map(|c| c.id())
            .collect();
        self.expansion_depth += 1;
        let start = self.items.diagnostics.len();
        self.collect_items(
            Ctx {
                module: ctx.module,
                file,
            },
            &items,
        );
        self.tag(start, file);
        self.expansion_depth -= 1;
    }

    /// Imports have item scope even through modules collected later. Retry
    /// pending calls against completed namespaces with their original textual scope.
    fn collect_pending_items(&mut self, from: super::ItemCounts) {
        let base_map = std::mem::take(&mut self.items.def_map);
        let saved_scope = self.macro_scope;
        let saved_depth = self.expansion_depth;
        while !self.pending_items.is_empty() {
            self.items.def_map = base_map.clone();
            let diagnostics = std::mem::take(&mut self.items.diagnostics);
            let imports = self.items.macro_imports.clone();
            crate::def_map::resolve_namespaces(self.items, from);
            self.items.diagnostics = diagnostics;
            self.items.macro_imports = imports;
            let pending = std::mem::take(&mut self.pending_items);
            let mut unresolved = Vec::new();
            let mut progress = false;
            for call in pending {
                let shared = self.db.shared(call.ctx.file);
                let node = shared.parse.tree().node(call.node);
                self.macro_scope = call.scope;
                let resolved = match call.kind {
                    PendingItemKind::Attribute => crate::cfg::Attrs::new(node.children().filter_map(ast::Attr::cast)).iter().find_map(|attr| {
                        let path = attr.meta()?.path()?;
                        let mut tokens = Vec::new();
                        macros::tt::tokens_of(path.syntax(), &mut tokens);
                        self.proc_path(call.ctx, &tokens).filter(|&id| matches!(&self.items.macros[id].kind, macros::MacroKind::ProcMacro(host) if host.kind == crate::ProcMacroKind::Attribute))
                    }),
                    PendingItemKind::Macro(_) => {
                        let mc = ast::MacroCall::cast(node).expect("pending macro call");
                        mc.path().and_then(|path| match macros::resolve::<()>(self.items, path, &[], call.scope, call.ctx.module).ok()? {
                            macros::Resolved::Def(id) => Some(id),
                            macros::Resolved::Local(_) => None,
                        })
                    }
                };
                match resolved {
                    Some(def) => {
                        progress = true;
                        self.macro_scope = call.scope;
                        self.expansion_depth = call.depth;
                        let _file = TypesFile::enter(call.ctx.file);
                        let start = self.items.diagnostics.len();
                        match call.kind {
                            PendingItemKind::Macro(owner) => {
                                let mc = ast::MacroCall::cast(node).expect("pending macro call");
                                if let Some(owner) = owner {
                                    self.collect_assoc_macro(call.ctx, owner, mc);
                                } else {
                                    self.collect_resolved_macro_call(call.ctx, mc, def);
                                }
                            }
                            PendingItemKind::Attribute => {
                                self.collect_proc_item(call.ctx, call.node);
                            }
                        }
                        self.tag(start, call.ctx.file);
                    }
                    None => unresolved.push(call),
                }
            }
            if !progress {
                for call in unresolved {
                    let start = self.items.diagnostics.len();
                    if let Some(diagnostic) = call.diagnostic {
                        self.items.diagnostics.push(diagnostic);
                    } else {
                        self.macro_scope = call.scope;
                        self.expansion_depth = call.depth;
                        self.collect_item_expanding_types(call.ctx, call.node);
                        self.collect_anon_consts(call.ctx, call.node);
                    }
                    self.tag(start, call.ctx.file);
                }
                break;
            }
            self.pending_items.extend(unresolved);
        }
        self.items.def_map = base_map;
        self.macro_scope = saved_scope;
        self.expansion_depth = saved_depth;
    }

    fn collect_pending_body_macros(&mut self, from: super::ItemCounts) {
        if self.pending_body_macros.is_empty() {
            return;
        }
        let base_map = std::mem::take(&mut self.items.def_map);
        let saved_scope = self.macro_scope;
        let saved_depth = self.expansion_depth;
        while !self.pending_body_macros.is_empty() {
            // Preview namespaces only: impl registration is performed once by
            // final resolution. Keep existing errors and consumed macro imports.
            // Genuine external import metadata loads remain committed.
            self.items.def_map = base_map.clone();
            let diagnostics = std::mem::take(&mut self.items.diagnostics);
            let imports = self.items.macro_imports.clone();
            crate::def_map::resolve_namespaces(self.items, from);
            self.items.diagnostics = diagnostics;
            self.items.macro_imports = imports;
            let pending = std::mem::take(&mut self.pending_body_macros);
            let mut progress = false;
            for call in pending {
                let key = (call.ctx.file, call.node);
                if self.items.body_expansions.contains_key(&key) {
                    continue;
                }
                self.macro_scope = call.scope;
                self.expansion_depth = call.depth;
                let shared = self.db.shared(call.ctx.file);
                let _file = TypesFile::enter(call.ctx.file);
                self.collect_body_node(
                    call.ctx,
                    shared.parse.tree().node(call.node),
                    call.function,
                );
                progress |= self.items.body_expansions.contains_key(&key);
            }
            // Unknown macros retain their ordinary body-lowering diagnostics.
            // Retrying without any newly collected expansion cannot reveal more.
            if !progress {
                self.pending_body_macros.clear();
                break;
            }
        }
        self.items.def_map = base_map;
        self.macro_scope = saved_scope;
        self.expansion_depth = saved_depth;
    }

    fn visibility(&mut self, ctx: Ctx, vis: Option<ast::Visibility<'_>>) -> Visibility {
        // An item in a block is private to the module the block is in, as
        // in rustc: blocks scope names, not privacy.
        let ctx = Ctx {
            module: self.items.named_module(ctx.module),
            ..ctx
        };
        let private = Visibility::Module(ctx.module);
        let Some(vis) = vis else {
            return private;
        };
        if vis.l_paren_token().is_none() {
            return Visibility::Public;
        }
        let module = &self.items.modules[ctx.module];
        if vis.crate_token().is_some() {
            Visibility::Crate(module.krate)
        } else if vis.self_token().is_some() {
            private
        } else if vis.super_token().is_some() {
            match module.parent {
                Some(parent) => Visibility::Module(parent),
                None => {
                    let range = vis
                        .super_token()
                        .map_or(vis.syntax().text_range(), |t| t.text_range());
                    let message = "too many leading `super` keywords within `crate`";
                    self.items.diagnostics.push(Diagnostic::new(range, message));
                    private
                }
            }
        } else {
            self.restricted_visibility(ctx, vis).unwrap_or(private)
        }
    }

    /// `pub(in path)`: visible in an ancestor module, named by `path`.
    fn restricted_visibility(&mut self, ctx: Ctx, vis: ast::Visibility<'_>) -> Option<Visibility> {
        let path = lower_path(vis.path()?, &mut self.items.diagnostics)?;
        let whole = vis.path()?.syntax().text_range();
        let mut module = match path.anchor {
            PathAnchor::Crate => self.items.module_of(self.items.modules[ctx.module].krate),
            PathAnchor::DefinitionCrate(id) => self.items.module_of(id),
            PathAnchor::SelfModule => ctx.module,
            PathAnchor::Super(n) => {
                let mut module = ctx.module;
                for _ in 0..n {
                    module = self.items.modules[module].parent?;
                }
                module
            }
            // `pub(in self)`
            PathAnchor::Plain if path.as_name() == Some(kw::SelfLower) => {
                return Some(Visibility::Module(ctx.module));
            }
            PathAnchor::Plain | PathAnchor::Global
                if path
                    .edition
                    .unwrap_or_else(|| self.items.edition(ctx.module))
                    == flux_syntax::Edition::E2015 =>
            {
                self.items.module_of(self.items.modules[ctx.module].krate)
            }
            PathAnchor::Plain | PathAnchor::Global => {
                let message =
                    "relative paths are not supported in visibilities in 2018 edition or later";
                self.items.diagnostics.push(Diagnostic::new(whole, message));
                return None;
            }
        };
        for (i, &name) in path.segments.iter().enumerate() {
            let child = self
                .items
                .modules
                .iter()
                .find(|(_, m)| m.parent == Some(module) && m.name == name);
            match child {
                Some((id, _)) => module = id,
                None => {
                    let message =
                        format!("cannot find `{name}` in {}", self.items.module_desc(module));
                    let range = path.range(i, whole);
                    self.items.diagnostics.push(Diagnostic::new(range, message));
                    return None;
                }
            }
        }
        let vis = Visibility::Module(module);
        if !self.items.is_visible(vis, ctx.module) {
            let message = "visibilities can only be restricted to ancestor modules";
            self.items.diagnostics.push(Diagnostic::new(whole, message));
            return None;
        }
        Some(vis)
    }

    /// The visibility of an associated item: as written in an inherent
    /// `impl` block, and the trait's in a trait or its `impl` blocks,
    /// where none may be written.
    fn item_visibility(
        &mut self,
        ctx: Ctx,
        vis: Option<ast::Visibility<'_>>,
        owner: Option<Owner>,
    ) -> Visibility {
        let trait_vis = match owner {
            Some(Owner::Trait(t)) => self.items.traits[t].loc.vis,
            Some(Owner::Impl(imp)) if self.items.impls[imp].trait_ref.is_some() => {
                Visibility::Public
            }
            _ => return self.visibility(ctx, vis),
        };
        if let Some(vis) = vis {
            self.items.diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        trait_vis
    }

    /// `type Pair<T> = (T, T);`, outside traits and `impl` blocks.
    fn collect_type_alias(&mut self, ctx: Ctx, t: ast::TypeAlias<'_>) {
        check_attrs(t.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, t.visibility());
        let Some(name) = t.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        if let Some(list) = t.type_bound_list() {
            diagnostics.push(Diagnostic::new(
                list.syntax().text_range(),
                "bounds on `type`s in this context have no effect",
            ));
        }
        // Bounds on its parameters are not enforced, as in rustc: they
        // are kept only to resolve their traits.
        let mut generics = collect_generics(
            t.generic_param_list(),
            t.where_clauses().next(),
            Defaults::Alias,
            diagnostics,
        );
        if let Some(clause) = t.where_clauses().nth(1) {
            let mut more = collect_generics(None, Some(clause), Defaults::No, diagnostics);
            let mut predicates = generics.predicates.into_vec();
            predicates.extend(std::mem::take(&mut more.predicates));
            generics.predicates = predicates.into();
        }
        let (ty, ty_range) = match t.ty() {
            Some(ty) => {
                let mut lowered = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::Unstable("type aliases");
                reject_impl_trait(&mut lowered, site, diagnostics);
                (lowered, ty.syntax().text_range())
            }
            None => {
                diagnostics.push(Diagnostic::new(item_range, "free type alias without body"));
                (TypeRef::Error, item_range)
            }
        };
        let name_range = name.syntax().text_range();
        self.items.type_aliases.alloc(TypeAlias {
            name: Symbol::intern(name.text()),
            generics,
            ty,
            ty_range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(t.syntax()), name_range.end()),
        });
    }

    /// `type Out: Bound;` in trait `id`.
    fn collect_trait_type(&mut self, id: TraitId, t: ast::TypeAlias<'_>) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(t.attrs(), diagnostics);
        let Some(name) = t.name() else {
            return;
        };
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        if let Some(vis) = t.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        let generics = associated_generics(t, diagnostics);
        if t.ty().is_some() {
            diagnostics.push(Diagnostic::new(
                item_range,
                "associated type defaults are unstable",
            ));
        }
        let mut bounds = Vec::new();
        let mut relaxed = Vec::new();
        let mut lifetime_bounds = Vec::new();
        if let Some(list) = t.type_bound_list() {
            lower_bounds(
                list,
                &mut bounds,
                Some(&mut lifetime_bounds),
                Relaxed::Collect(&mut relaxed),
                diagnostics,
            );
        }
        if relaxed.len() > 1 {
            diagnostics.push(Diagnostic::new(
                relaxed[1],
                "duplicate relaxed `Sized` bounds",
            ));
        }
        let maybe_unsized = !relaxed.is_empty();
        for bound in &mut bounds {
            reject_impl_trait_in_path(&mut bound.path, IN_BOUNDS, diagnostics);
        }
        let name_range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let data = &mut self.items.traits[id];
        if data.types.iter().any(|ty| ty.name == name) {
            diagnostics.push(Diagnostic::new(
                item_range,
                format!("the name `{name}` is defined multiple times"),
            ));
            return;
        }
        data.types.push(TraitType {
            maybe_unsized,
            name,
            generics,
            bounds: bounds.into(),
            lifetime_bounds: lifetime_bounds.into(),
            name_range,
            item_range,
        });
    }

    /// `type Out = T;` in `impl` block `id`.
    fn collect_impl_type(&mut self, id: ImplId, t: ast::TypeAlias<'_>, is_trait_impl: bool) {
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(t.attrs(), diagnostics);
        let item_range = TextRange::new(def_start(t.syntax()), t.syntax().text_range().end());
        let Some(name) = t.name() else {
            return;
        };
        if !is_trait_impl {
            diagnostics.push(Diagnostic::new(
                item_range,
                "inherent associated types are unstable",
            ));
            return;
        }
        if let Some(vis) = t.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        let generics = associated_generics(t, diagnostics);
        if let Some(list) = t.type_bound_list() {
            diagnostics.push(Diagnostic::new(
                list.syntax().text_range(),
                "bounds on `type`s in `impl`s have no effect",
            ));
        }
        let (ty, ty_range) = match t.ty() {
            Some(ty) => {
                let range = ty.syntax().text_range();
                let mut lowered = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::Unstable("associated types");
                reject_impl_trait(&mut lowered, site, diagnostics);
                (lowered, range)
            }
            None => {
                // Reported; defined too.
                diagnostics.push(Diagnostic::new(
                    item_range,
                    "associated type in `impl` without body",
                ));
                (TypeRef::Error, item_range)
            }
        };
        let name_range = name.syntax().text_range();
        let name = Symbol::intern(name.text());
        let imp = &mut self.items.impls[id];
        if imp.types.iter().any(|t| t.name == name) {
            diagnostics.push(Diagnostic::new(
                item_range,
                format!("duplicate definitions with name `{name}`:"),
            ));
            return;
        }
        imp.types.push(ImplType {
            name,
            generics,
            ty,
            ty_range,
            name_range,
            item_range,
        });
    }

    fn collect_trait(&mut self, ctx: Ctx, t: ast::Trait<'_>) {
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let mut on_unimplemented = None;
        let attrs = crate::cfg::Attrs::new(t.attrs());
        for attr in attrs.iter() {
            if let Some(message) = on_unimplemented_attr(attr) {
                on_unimplemented = message;
                continue;
            }
            match lang_attr(attr) {
                Some(name) if is_core => lang = Some(name),
                _ => check_attr(attr, &mut self.items.diagnostics),
            }
        }
        let vis = self.visibility(ctx, t.visibility());
        let Some(name) = t.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let mut generics = collect_generics(
            t.generic_param_list(),
            t.where_clause(),
            Defaults::Trait,
            diagnostics,
        );
        let mut supertraits = Vec::new();
        let mut lifetimes = Vec::new();
        if let Some(list) = t.type_bound_list() {
            lower_bounds(
                list,
                &mut supertraits,
                Some(&mut lifetimes),
                Relaxed::Supertrait,
                diagnostics,
            );
        }
        let mut type_outlives = generics.type_outlives.into_vec();
        type_outlives.extend(lifetimes.into_iter().map(|lifetime| TypeOutlives {
            ty: TypeRef::Path(Path::single(kw::SelfUpper)),
            ty_range: name.syntax().text_range(),
            lifetime,
        }));
        generics.type_outlives = type_outlives.into();
        for bound in &mut supertraits {
            reject_impl_trait_in_path(&mut bound.path, IN_BOUNDS, diagnostics);
        }
        let name_range = name.syntax().text_range();
        let id = self.items.traits.alloc(Trait {
            name: Symbol::intern(name.text()),
            generics,
            supertraits: supertraits.into(),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            is_unsafe: t.unsafe_token().is_some(),
            lang,
            on_unimplemented,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
            name_range,
            def_range: TextRange::new(def_start(t.syntax()), name_range.end()),
        });
        if let Some(lang) = lang {
            self.items.lang_traits.insert(lang, id);
        }
        let Some(list) = t.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        let nodes: Vec<_> = list.items().map(|item| item.syntax().id()).collect();
        self.collect_trait_items(ctx, id, &nodes);
    }

    fn collect_trait_items(&mut self, ctx: Ctx, id: TraitId, nodes: &[NodeId]) {
        let _file = TypesFile::enter(ctx.file);
        let shared = self.db.shared(ctx.file);
        for &node in nodes {
            let Some(item) = ast::Item::cast(shared.parse.tree().node(node)) else {
                continue;
            };
            if !item_enabled(item.syntax(), &mut self.items.diagnostics) {
                continue;
            }
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, None, Some(Owner::Trait(id))) {
                        let data = &self.items.functions[function];
                        let duplicate =
                            self.items.traits[id].value(self.items, data.name).is_some();
                        if duplicate {
                            // rustc's span has a bodiless item's `;`.
                            let range = if data.has_body {
                                data.def_range
                            } else {
                                data.item_range
                            };
                            let name = data.name;
                            self.items.diagnostics.push(Diagnostic::new(
                                range,
                                format!("the name `{name}` is defined multiple times"),
                            ));
                        } else {
                            self.items.traits[id].functions.push(function);
                            let name = self.items.functions[function].name;
                            self.items.trait_fns.entry(name).or_default().push(id);
                        }
                    }
                    continue;
                }
                ast::Item::Const(c) => {
                    let Some(konst) = self.collect_const(ctx, c, Some(Owner::Trait(id))) else {
                        continue;
                    };
                    let data = &self.items.consts[konst];
                    let Some(name) = data.name else {
                        continue;
                    };
                    if self.items.traits[id].value(self.items, name).is_some() {
                        self.items.diagnostics.push(Diagnostic::new(
                            data.item_range,
                            format!("the name `{name}` is defined multiple times"),
                        ));
                    } else {
                        self.items.traits[id].consts.push(konst);
                        self.items.trait_fns.entry(name).or_default().push(id);
                    }
                    continue;
                }
                ast::Item::TypeAlias(t) => {
                    self.collect_trait_type(id, t);
                    continue;
                }
                ast::Item::MacroCall(call) => {
                    self.collect_assoc_macro(ctx, AssocOwner::Trait(id), call);
                    continue;
                }
                _ => "items like this in traits",
            };
            unsupported(
                &mut self.items.diagnostics,
                item.syntax().text_range(),
                what,
            );
        }
    }

    fn collect_module(&mut self, ctx: Ctx, m: ast::Module<'_>) {
        // Macros defined in a module are in scope until its end, or
        // after it too with `#[macro_use]`.
        let mut macro_use = false;
        let mut path = None;
        let mut invalid_path = false;
        let attrs = crate::cfg::Attrs::new(m.attrs());
        for attr in attrs.iter() {
            if attr_name(attr) == Some("macro_use") {
                if attr.meta().is_some_and(|m| {
                    m.token_tree().is_none() && m.eq_token().is_none() && m.unsafe_token().is_none()
                }) {
                    macro_use = true;
                } else {
                    self.items.diagnostics.push(Diagnostic::new(
                        attr.syntax().text_range(),
                        "arguments to `macro_use` are not allowed here",
                    ));
                }
            } else if attr_name(attr) == Some("path") {
                let range = attr.syntax().text_range();
                let message = if path.is_some() {
                    Some("duplicate path attributes are not supported yet")
                } else if m.item_list().is_some() {
                    Some("path attributes on inline modules are not supported yet")
                } else if self.db.file(ctx.file).expansion_of.is_some() {
                    Some("path attributes in item macro expansions are not supported yet")
                } else if self.items.modules[ctx.module].block.is_some()
                    || m.syntax().parent().is_none_or(|p| {
                        !matches!(p.kind(), SyntaxKind::SOURCE_FILE | SyntaxKind::MACRO_ITEMS)
                    })
                {
                    Some("path attributes inside inline modules or blocks are not supported yet")
                } else {
                    None
                };
                if let Some(message) = message {
                    self.items.diagnostics.push(Diagnostic::new(range, message));
                    invalid_path = true;
                    continue;
                }
                let literal = attr
                    .meta()
                    .filter(|meta| {
                        meta.unsafe_token().is_none()
                            && meta.eq_token().is_some()
                            && meta.token_tree().is_none()
                    })
                    .and_then(|meta| meta.expr())
                    .and_then(|expr| match expr {
                        ast::Expr::Literal(literal) => Some(literal),
                        _ => None,
                    })
                    .and_then(|literal| literal.value())
                    .filter(|token| token.kind() == SyntaxKind::STRING);
                match literal.map(|token| crate::literal::unescape_str(token.text())) {
                    Some(Ok(value)) => path = Some(PathBuf::from(value)),
                    value => {
                        let message = value
                            .and_then(Result::err)
                            .unwrap_or_else(|| "expected a string literal in `path`".into());
                        self.items.diagnostics.push(Diagnostic::new(range, message));
                        invalid_path = true;
                    }
                }
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        if invalid_path {
            return;
        }
        let outer_scope = self.macro_scope;
        self.collect_module_inner(ctx, m, path, &mut macro_use);
        if !macro_use {
            self.macro_scope = outer_scope;
        }
    }

    fn collect_module_inner(
        &mut self,
        ctx: Ctx,
        m: ast::Module<'_>,
        path: Option<PathBuf>,
        macro_use: &mut bool,
    ) {
        let vis = self.visibility(ctx, m.visibility());
        let Some(name) = m.name() else {
            return;
        };
        let name_text = name.text();
        let parent = &self.items.modules[ctx.module];
        if parent.block.is_some() && m.item_list().is_none() {
            let range = TextRange::new(def_start(m.syntax()), m.syntax().text_range().end());
            self.items.diagnostics.push(Diagnostic::new(
                range,
                "cannot declare a file module inside a block unless it has a path attribute",
            ));
            return;
        }
        let def_range = TextRange::new(def_start(m.syntax()), name.syntax().text_range().end());
        let source = self.db.file(ctx.file);
        let parent_dir = if source.range_base != 0 && parent.file != ctx.file {
            source.path.parent().unwrap_or_else(|| FsPath::new(""))
        } else {
            &parent.dir
        };
        let mut child_dir = parent_dir.join(name_text);
        let krate = parent.krate;
        // A literal path is relative to the containing source file, including
        // in a normal foo.rs module (whose default child directory is foo/).
        let path = path.map(|path| {
            self.db
                .file(self.db.real_file(ctx.file))
                .path
                .parent()
                .unwrap_or_else(|| FsPath::new(""))
                .join(path)
        });
        let file = if m.item_list().is_none() {
            let path = if let Some(path) = path {
                // Explicitly named files own their containing directory, even
                // when the target is not named mod.rs or the module is renamed.
                child_dir = path
                    .parent()
                    .unwrap_or_else(|| FsPath::new(""))
                    .to_path_buf();
                path
            } else {
                let flat = parent_dir.join(format!("{name_text}.rs"));
                let nested = child_dir.join("mod.rs");
                match (self.db.exists(&flat), self.db.exists(&nested)) {
                    (true, false) => flat,
                    (false, true) => nested,
                    (true, true) => {
                        let message = format!(
                            "file for module `{name_text}` found at both \"{}\" and \"{}\"",
                            flat.display(),
                            nested.display()
                        );
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(def_range, message));
                        return;
                    }
                    (false, false) => {
                        self.items.diagnostics.push(Diagnostic::new(
                            def_range,
                            format!("file not found for module `{name_text}`"),
                        ));
                        return;
                    }
                }
            };
            // Compare physical paths only for recursion detection. Keep the
            // requested path and unmodified bytes in SourceDb for diagnostics.
            let physical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let mut ancestor = Some(ctx.module);
            let mut depth = 0;
            while let Some(id) = ancestor {
                let module = &self.items.modules[id];
                let source = &self.db.file(self.db.real_file(module.file)).path;
                let source = std::fs::canonicalize(source).unwrap_or_else(|_| source.clone());
                depth += 1;
                if source == physical || depth >= 128 {
                    self.items.diagnostics.push(Diagnostic::new(
                        def_range,
                        "recursive module loading or module nesting limit reached",
                    ));
                    return;
                }
                ancestor = module.parent;
            }
            let file = match self
                .db
                .load_with_edition(&path, self.db.file(ctx.file).edition)
            {
                Ok(file) => file,
                Err(message) => {
                    self.items
                        .diagnostics
                        .push(Diagnostic::new(def_range, message));
                    return;
                }
            };
            let cfg = self.items.crates[krate.0 as usize].cfg.clone();
            let file = self.db.configured_file(file, cfg);
            // Target inner cfg must run before allocating a visible name.
            let start = self.items.diagnostics.len();
            let shared = self.db.shared(file);
            let enabled = item_enabled(shared.parse.root(), &mut self.items.diagnostics);
            self.tag(start, file);
            if !enabled {
                return;
            }
            Some(file)
        } else {
            None
        };
        let module = self.items.modules.alloc(Module {
            name: Symbol::intern(name_text),
            parent: Some(ctx.module),
            krate,
            file: file.unwrap_or(ctx.file),
            dir: child_dir,
            vis,
            def_range,
            block: None,
        });
        if let Some(list) = m.item_list() {
            *macro_use |= crate::cfg::Attrs::new(list.attrs()).iter().any(|attr| {
                attr_name(attr) == Some("macro_use") && super::attributes::bare_attr(attr)
            });
            check_inner_attrs(list.attrs(), false, false, &mut self.items.diagnostics);
            let items: Vec<NodeId> = list.items().map(|i| i.syntax().id()).collect();
            self.collect_items(
                Ctx {
                    module,
                    file: ctx.file,
                },
                &items,
            );
        } else if let Some(file) = file {
            let shared = self.db.shared(file);
            let source = ast::SourceFile::cast(shared.parse.root()).expect("source file root");
            *macro_use |= crate::cfg::Attrs::new(source.attrs()).iter().any(|attr| {
                attr_name(attr) == Some("macro_use") && super::attributes::bare_attr(attr)
            });
            self.collect_file(module, file);
        }
    }

    fn collect_use(&mut self, ctx: Ctx, u: ast::Use<'_>) {
        check_attrs(u.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, u.visibility());
        if let Some(tree) = u.use_tree() {
            let prefix = Path {
                edition: None,
                anchor: PathAnchor::Plain,
                qself: None,
                segments: Box::default(),
                ranges: Box::default(),
                anchor_ranges: Box::default(),
                args: Box::default(),
            };
            self.collect_use_tree(ctx, vis, &prefix, tree);
        }
    }

    /// Flattens `use a::{b, c::d as e, f::*}` into one import per name.
    fn collect_use_tree(
        &mut self,
        ctx: Ctx,
        vis: Visibility,
        prefix: &Path,
        tree: ast::UseTree<'_>,
    ) {
        let range = tree.syntax().text_range();
        let mut path = prefix.clone();
        if let Some(p) = tree.path() {
            let Some(mut p) = lower_path(p, &mut self.items.diagnostics) else {
                return;
            };
            // `self` alone is a binding in expressions and at the end of
            // imports, but `use self::{...}` anchors the entire group.
            if tree.use_tree_list().is_some()
                && prefix.anchor == PathAnchor::Plain
                && prefix.segments.is_empty()
                && p.anchor == PathAnchor::Plain
                && *p.segments == [kw::SelfLower]
            {
                p.anchor = PathAnchor::SelfModule;
                p.segments = Box::default();
                p.ranges = Box::default();
            }
            if prefix.segments.is_empty() && prefix.anchor == PathAnchor::Plain {
                path = p;
            } else if p.anchor == PathAnchor::Plain {
                let segments: Vec<Symbol> = prefix
                    .segments
                    .iter()
                    .chain(p.segments.iter())
                    .copied()
                    .collect();
                let ranges: Vec<TextRange> = prefix
                    .ranges
                    .iter()
                    .chain(p.ranges.iter())
                    .copied()
                    .collect();
                path.segments = segments.into();
                path.ranges = ranges.into();
            } else {
                let message = "`crate`, `self` and `super` can only start a path";
                self.items.diagnostics.push(Diagnostic::new(range, message));
                return;
            }
        }
        if tree.star_token().is_some() {
            if path
                .edition
                .unwrap_or_else(|| self.items.edition(ctx.module))
                == flux_syntax::Edition::E2015
                && path.anchor == PathAnchor::Plain
            {
                path.anchor = PathAnchor::Global;
            }
            self.items.uses.push(Use {
                module: ctx.module,
                file: ctx.file,
                vis,
                path,
                kind: UseKind::Glob,
                range,
            });
            return;
        }
        if let Some(list) = tree.use_tree_list() {
            for child in list.use_trees() {
                self.collect_use_tree(ctx, vis, &path, child);
            }
            return;
        }
        if path
            .edition
            .unwrap_or_else(|| self.items.edition(ctx.module))
            == flux_syntax::Edition::E2015
            && path.anchor == PathAnchor::Plain
        {
            path.anchor = PathAnchor::Global;
        }
        // `use m::{self};` imports the module `m`.
        let module_only = path.segments.last() == Some(&kw::SelfLower);
        if module_only {
            let mut segments = path.segments.to_vec();
            segments.pop();
            path.segments = segments.into();
            let mut ranges = path.ranges.to_vec();
            ranges.pop();
            path.ranges = ranges.into();
        }
        let anonymous = tree
            .rename()
            .is_some_and(|rename| rename.underscore_token().is_some());
        let rename = match tree.rename() {
            Some(rename) => rename.name().map(|name| Symbol::intern(name.text())),
            None => None,
        };
        let Some(name) = rename.or_else(|| path.segments.last().copied()) else {
            let message = "imports need to be explicitly named";
            self.items.diagnostics.push(Diagnostic::new(range, message));
            return;
        };
        let kind = if anonymous {
            UseKind::Anonymous { module_only }
        } else if module_only {
            UseKind::Module(name)
        } else {
            UseKind::Named(name)
        };
        // `use m;` of a `macro_rules!` macro in textual scope makes it
        // nameable by path, as `crate::m!` and so on.
        if let (UseKind::Named(name), PathAnchor::Plain, [only]) =
            (kind, path.anchor, &path.segments[..])
            && let Some(def) = self.items.textual_macro(self.macro_scope, *only)
        {
            self.items.macro_imports.push(MacroImport {
                module: ctx.module,
                name,
                def,
                vis,
                file: ctx.file,
                range,
            });
            return;
        }
        self.items.uses.push(Use {
            module: ctx.module,
            file: ctx.file,
            vis,
            path,
            kind,
            range,
        });
    }

    fn collect_impl(&mut self, ctx: Ctx, imp: ast::Impl<'_>) {
        let root = self.items.crates[self.items.modules[ctx.module].krate.0 as usize].root;
        let file = self.items.modules[root].file;
        let trusted_drop =
            [FileId::CORE_ROOT, FileId::alloc_root(), FileId::std_root()].contains(&file);
        let diagnostics = &mut self.items.diagnostics;
        check_attrs(imp.attrs(), diagnostics);
        if let Some(bang) = imp.bang_token() {
            diagnostics.push(Diagnostic::new(
                bang.text_range(),
                "negative impls are not supported yet",
            ));
            return;
        }
        let range = imp.syntax().text_range();
        if let Some(vis) = imp.visibility() {
            diagnostics.push(Diagnostic::new(
                vis.syntax().text_range(),
                "visibility qualifiers are not permitted here",
            ));
        }
        let generics = collect_generics_with_drop(
            imp.generic_param_list(),
            imp.where_clause(),
            Defaults::No,
            trusted_drop,
            diagnostics,
        );
        if (generics.params.iter().any(|p| p.may_dangle)
            || generics.lifetimes.iter().any(|p| p.may_dangle))
            && (imp.unsafe_token().is_none() || imp.trait_ty().is_none())
        {
            diagnostics.push(Diagnostic::new(
                range,
                "`may_dangle` requires an unsafe `Drop` impl",
            ));
        }
        let trait_ref = match imp.trait_ty() {
            Some(ty) => match lower_bound_type(ty, diagnostics) {
                Some(mut bound) => {
                    let site = ImplTraitSite::NotAllowed("traits");
                    reject_impl_trait_in_path(&mut bound.path, site, diagnostics);
                    Some(bound)
                }
                None => return,
            },
            // `impl for S`, reported by the parser.
            None if imp.for_token().is_some() => return,
            None => None,
        };
        if trait_ref.is_none()
            && let Some(token) = imp.unsafe_token()
        {
            diagnostics.push(Diagnostic::new(
                token.text_range(),
                "inherent impls cannot be unsafe",
            ));
        }
        let mut self_ty = lower_type(imp.self_ty(), diagnostics);
        reject_impl_trait(
            &mut self_ty,
            ImplTraitSite::NotAllowed("impl headers"),
            diagnostics,
        );
        let self_range = imp.self_ty().map_or(range, |t| t.syntax().text_range());
        let header_end = imp
            .where_clause()
            .map_or(self_range.end(), |w| w.syntax().text_range().end());
        let def_range = TextRange::new(def_start(imp.syntax()), header_end);
        let is_trait_impl = trait_ref.is_some();
        let id = self.items.impls.alloc(Impl {
            generics,
            trait_ref,
            trait_: None,
            is_unsafe: imp.unsafe_token().is_some(),
            self_ty,
            self_range,
            def_range,
            module: ctx.module,
            file: ctx.file,
            target: None,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
        });
        self.impl_ids.insert((ctx.file, imp.syntax().id()), id);
        let Some(list) = imp.assoc_item_list() else {
            return;
        };
        check_attrs(list.attrs(), &mut self.items.diagnostics);
        let nodes: Vec<_> = list.items().map(|item| item.syntax().id()).collect();
        self.collect_impl_items(ctx, id, &nodes, is_trait_impl);
    }

    fn collect_impl_items(&mut self, ctx: Ctx, id: ImplId, nodes: &[NodeId], is_trait_impl: bool) {
        let _file = TypesFile::enter(ctx.file);
        let shared = self.db.shared(ctx.file);
        for &node in nodes {
            let Some(item) = ast::Item::cast(shared.parse.tree().node(node)) else {
                continue;
            };
            if !item_enabled_with_test_context(
                item.syntax(),
                &mut self.items.diagnostics,
                Some(false),
            ) {
                continue;
            }
            let what = match item {
                ast::Item::Fn(f) => {
                    if let Some(function) = self.collect_fn(ctx, f, None, Some(Owner::Impl(id))) {
                        let (name, range) = {
                            let data = &self.items.functions[function];
                            (data.name, data.item_range)
                        };
                        if !(is_trait_impl && self.duplicate_value(id, name, range)) {
                            self.items.impls[id].functions.push(function);
                        }
                    }
                    continue;
                }
                ast::Item::Const(c) => {
                    if let Some(konst) = self.collect_const(ctx, c, Some(Owner::Impl(id))) {
                        let (name, range) = {
                            let data = &self.items.consts[konst];
                            (data.name, data.item_range)
                        };
                        let duplicate = name.is_some_and(|name| {
                            is_trait_impl && self.duplicate_value(id, name, range)
                        });
                        if !duplicate {
                            self.items.impls[id].consts.push(konst);
                        }
                    }
                    continue;
                }
                ast::Item::TypeAlias(t) => {
                    self.collect_impl_type(id, t, is_trait_impl);
                    continue;
                }
                ast::Item::MacroCall(call) => {
                    self.collect_assoc_macro(ctx, AssocOwner::Impl(id, is_trait_impl), call);
                    continue;
                }
                _ => "items like this in `impl` blocks",
            };
            unsupported(
                &mut self.items.diagnostics,
                item.syntax().text_range(),
                what,
            );
        }
    }

    /// Associated items from a macro keep the surrounding impl or trait
    /// owner. They must not enter the module namespace as ordinary item
    /// macros do.
    fn collect_assoc_macro(&mut self, ctx: Ctx, owner: AssocOwner, call: ast::MacroCall<'_>) {
        check_attrs(call.attrs(), &mut self.items.diagnostics);
        let (owner_name, position) = match owner {
            AssocOwner::Impl(..) => ("impls", "non-impl item macro in impl item position"),
            AssocOwner::Trait(_) => ("traits", "non-trait item macro in trait item position"),
        };
        if macros::is_macro_rules(call.syntax()) {
            self.items.diagnostics.push(Diagnostic::new(
                call.syntax().text_range(),
                format!("macro definitions are not supported in {owner_name}"),
            ));
            return;
        }
        let Some(path) = call.path() else { return };
        let def = match macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module) {
            Ok(macros::Resolved::Def(id)) => id,
            Ok(macros::Resolved::Local(_)) => unreachable!(),
            Err(error) => {
                self.pending_items.push(PendingItem {
                    kind: PendingItemKind::Macro(Some(owner)),
                    ctx,
                    node: call.syntax().id(),
                    scope: self.macro_scope,
                    depth: self.expansion_depth,
                    diagnostic: Some(error),
                });
                return;
            }
        };
        self.record_macro_path(ctx, path, def);
        let def = self.items.macros[def].clone();
        if matches!(
            def.kind,
            macros::MacroKind::Builtin(macros::Builtin::Include)
        ) {
            self.items.diagnostics.push(Diagnostic::new(
                call.syntax().text_range(),
                format!("{position}: include"),
            ));
            return;
        }
        if self.expansion_depth as usize >= self.items.macro_recursion_limit(ctx.module) {
            self.items.diagnostics.push(Diagnostic::new(
                call.syntax().text_range(),
                format!("recursion limit reached while expanding `{}!`", def.name),
            ));
            return;
        }
        let real = self.real_file(ctx.file);
        let site = macros::CallSite {
            read_text: crate::source::SourceDb::read_include_str,
            scope: self.macro_scope,
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: real,
        };
        let mut diagnostics = Vec::new();
        let expansion = macros::expand(
            &def,
            call.syntax(),
            &site,
            flux_syntax::parser::Fragment::Items,
            &mut diagnostics,
        );
        self.items.diagnostics.extend(diagnostics);
        let Some(expansion) = expansion else { return };
        let file = if let Some(path) = expansion.included_path {
            match self
                .db
                .add_inclusion(expansion.parse, path, self.db.file(ctx.file).edition)
            {
                Ok(file) => file,
                Err(message) => {
                    self.items
                        .diagnostics
                        .push(Diagnostic::new(call.syntax().text_range(), message));
                    return;
                }
            }
        } else {
            let file = self.db.add_expansion(expansion.parse, real);
            self.items.expansion_files.insert(file, real);
            file
        };
        let shared = self.db.shared(file);
        self.items.insert_file(file, Arc::clone(&shared));
        let nodes: Vec<_> = shared
            .parse
            .tree()
            .root()
            .children()
            .filter(|node| is_item_kind(node.kind()))
            .map(|node| node.id())
            .collect();
        self.expansion_depth += 1;
        let start = self.items.diagnostics.len();
        with_macro_types(|| {
            let ctx = Ctx { file, ..ctx };
            let mut diagnostics = Vec::new();
            expand_types(
                shared.parse.tree().root(),
                &mut diagnostics,
                &mut |call, diagnostics| self.expand_type_call(ctx, call, diagnostics),
            );
            self.items.diagnostics.extend(diagnostics);
            match owner {
                AssocOwner::Impl(id, is_trait_impl) => {
                    self.collect_impl_items(ctx, id, &nodes, is_trait_impl);
                }
                AssocOwner::Trait(id) => self.collect_trait_items(ctx, id, &nodes),
            }
        });
        self.tag(start, file);
        self.expansion_depth -= 1;
    }

    /// Whether trait `impl` block `id` already has a function or constant
    /// called `name`, reported at `range` (E0201): they share a namespace.
    fn duplicate_value(&mut self, id: ImplId, name: Symbol, range: TextRange) -> bool {
        let imp = &self.items.impls[id];
        let items = &self.items;
        let exists = imp
            .functions
            .iter()
            .any(|&f| items.functions[f].name == name)
            || imp
                .consts
                .iter()
                .any(|&c| items.consts[c].name == Some(name));
        if exists {
            self.items.diagnostics.push(Diagnostic::new(
                range,
                format!("duplicate definitions with name `{name}`:"),
            ));
        }
        exists
    }

    fn collect_fn(
        &mut self,
        ctx: Ctx,
        f: ast::Fn<'_>,
        extern_block: Option<Abi>,
        owner: Option<Owner>,
    ) -> Option<FunctionId> {
        let in_extern_block = extern_block.is_some();
        let is_core = self.items.is_core(ctx.module);
        let mut inherits_overflow_checks = false;
        let mut const_unstable = false;
        let mut precise_live_drops = false;
        let mut intrinsic = false;
        let mut sized_dispatch = None;
        let mut legacy_const_generic = None;
        let mut track_caller = false;
        let mut inline = None;
        let mut cold = false;
        let mut do_not_const_check = false;
        let mut lang = None;
        let mut link_name = None;
        let attrs = crate::cfg::Attrs::new(f.attrs());
        let has_target_feature = attrs.iter().any(|a| attr_name(a) == Some("target_feature"));
        let mut enabled_features = Vec::new();
        if has_target_feature
            && f.unsafe_token().is_none()
            && owner.is_some_and(|owner| match owner {
                Owner::Trait(_) => true,
                Owner::Impl(id) => self.items.impls[id].trait_ref.is_some(),
            })
        {
            self.items.diagnostics.push(Diagnostic::new(
                f.syntax().text_range(),
                "`target_feature` cannot be applied to safe trait methods",
            ));
        }
        for attr in attrs.iter() {
            if attr_name(attr) == Some("link_name") {
                let name = super::attributes::link_name_attr(attr, &mut self.items.diagnostics);
                if in_extern_block && link_name.is_none() {
                    link_name = name;
                }
                continue;
            }
            if attr_name(attr) == Some("target_feature") {
                enabled_features.extend(target_feature_attr(
                    attr,
                    f,
                    in_extern_block,
                    &mut self.items.diagnostics,
                ));
                continue;
            }
            if attr_name(attr) == Some("inline") {
                inline = super::attributes::inline_attr(attr).or(inline);
            } else if attr_name(attr) == Some("cold") {
                cold |= super::attributes::bare_attr(attr);
            }
            if has_target_feature && attr_name(attr) == Some("inline") {
                target_feature_inline(attr, &mut self.items.diagnostics);
                continue;
            }
            if attr_name(attr) == Some("track_caller")
                && attr.meta().is_some_and(|m| m.path().is_some())
            {
                track_caller = true;
                continue;
            }
            if is_core && attr_name(attr) == Some("rustc_inherit_overflow_checks") {
                inherits_overflow_checks = true;
            } else if is_core && attr_name(attr) == Some("rustc_const_unstable") {
                const_unstable = true;
            } else if is_core && attr_name(attr) == Some("rustc_allow_const_fn_unstable") {
                precise_live_drops = true;
            } else if (is_core || self.items.is_std(ctx.module))
                && attr_name(attr) == Some("rustc_intrinsic")
            {
                intrinsic = true;
            } else if (is_core || self.items.is_std(ctx.module))
                && attr_name(attr) == Some("rustc_sized_dispatch")
            {
                let parsed = super::sized_dispatch::parse(attr);
                if sized_dispatch.is_some() || parsed.is_none() {
                    self.items.diagnostics.push(Diagnostic::new(attr.syntax().text_range(),
                        "expected one selector index and two helper identifiers in sized dispatch metadata"));
                } else {
                    sized_dispatch = parsed;
                }
            } else if is_core && attr_name(attr) == Some("rustc_legacy_const_generics") {
                // The bounded convention used by our NEON wrappers. Keep the
                // position explicit rather than recognizing names at call sites.
                let tree = attr.meta().and_then(|m| m.token_tree());
                let tokens: Vec<_> = tree
                    .into_iter()
                    .flat_map(|t| t.syntax().tokens())
                    .filter(|t| !t.kind().is_trivia())
                    .collect();
                if let [l, n, r] = tokens.as_slice()
                    && l.kind() == SyntaxKind::L_PAREN
                    && n.kind() == SyntaxKind::INT_NUMBER
                    && r.kind() == SyntaxKind::R_PAREN
                    && let Ok(position) = n.text().parse::<u32>()
                {
                    legacy_const_generic = Some(position);
                } else {
                    unsupported(
                        &mut self.items.diagnostics,
                        attr.syntax().text_range(),
                        "malformed legacy const generic position",
                    );
                }
            } else if is_core && attr_name(attr) == Some("rustc_do_not_const_check") {
                do_not_const_check = true;
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.item_visibility(ctx, f.visibility(), owner);
        let name = f.name()?;
        let (outer, outer_lifetimes): (Vec<Symbol>, Vec<Symbol>) = match f.generic_param_list() {
            Some(_) => self
                .items
                .owner_generics(owner)
                .map_or_else(Default::default, |g| {
                    (
                        g.params.iter().map(|p| p.name).collect(),
                        g.lifetimes.iter().map(|l| l.name).collect(),
                    )
                }),
            None => Default::default(),
        };
        // Where the type parameters `impl Trait` parameter types make
        // start: after its owner's and its own.
        let owner_params = match owner {
            Some(Owner::Trait(t)) => 1 + self.items.traits[t].generics.params.len(),
            Some(Owner::Impl(imp)) => self.items.impls[imp].generics.params.len(),
            None => 0,
        };
        let in_trait_or_trait_impl = match owner {
            Some(Owner::Trait(_)) => true,
            Some(Owner::Impl(imp)) => self.items.impls[imp].trait_ref.is_some(),
            None => false,
        };
        let diagnostics = &mut self.items.diagnostics;
        let mut generics = collect_generics(
            f.generic_param_list(),
            f.where_clause(),
            Defaults::No,
            diagnostics,
        );
        if in_extern_block && !generics.is_empty() {
            let what = if generics.params.iter().all(GenericParam::is_const) {
                "const"
            } else {
                "type"
            };
            diagnostics.push(Diagnostic::new(
                TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
                format!("foreign items may not have {what} parameters"),
            ));
        }
        {
            // A method's parameters may not shadow its block's.
            for param in generics.params.iter() {
                if outer.contains(&param.name) {
                    diagnostics.push(Diagnostic::new(
                        param.range,
                        format!(
                            "the name `{}` is already used for a generic parameter in this item's generic parameters",
                            param.name
                        ),
                    ));
                }
            }
            for lifetime in generics.lifetimes.iter() {
                if outer_lifetimes.contains(&lifetime.name) {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        format!(
                            "lifetime name `{}` shadows a lifetime name that is already in scope",
                            lifetime.name
                        ),
                    ));
                }
            }
        }
        if let Some(token) = f.async_token() {
            unsupported(diagnostics, token.text_range(), "`async` functions");
        }
        if in_extern_block && let Some(abi) = f.abi() {
            diagnostics.push(Diagnostic::new(
                abi.syntax().text_range(),
                "functions in `extern` blocks cannot have an `extern` qualifier",
            ));
        }
        let abi = match extern_block {
            Some(abi) => abi,
            None => function_abi(f.abi(), diagnostics),
        };
        if abi.is_c() && track_caller {
            diagnostics.push(Diagnostic::new(
                f.syntax().text_range(),
                "`#[track_caller]` requires Rust ABI",
            ));
        }
        let mut params = Vec::new();
        let mut is_variadic = false;
        let mut has_self = false;
        let mut impl_params = ImplParams {
            first: (owner_params + generics.params.len()) as u32,
            ..ImplParams::default()
        };
        if let Some(list) = f.param_list() {
            if let Some(param) = list
                .self_param()
                .filter(|p| item_enabled(p.syntax(), diagnostics))
            {
                let range = param.syntax().text_range();
                check_inert_attrs(param.attrs(), diagnostics);
                if owner.is_none() {
                    diagnostics.push(Diagnostic::new(
                        range,
                        "`self` parameter is only allowed in associated functions",
                    ));
                } else {
                    let self_ty = TypeRef::Path(Path::single(kw::SelfUpper));
                    let ty = if let Some(ty) = param.ty() {
                        lower_type(Some(ty), diagnostics)
                    } else {
                        match param.amp_token() {
                            Some(_) => TypeRef::Ref {
                                lifetime: param.lifetime().map(lower_lifetime),
                                amp: param.amp_token().map_or(range, |t| t.text_range()),
                                mutable: param.mut_token().is_some(),
                                pointee: Box::new(self_ty),
                            },
                            None => self_ty,
                        }
                    };
                    params.push(Param {
                        ty,
                        range,
                        whole: range,
                        name: Some(kw::SelfLower),
                    });
                    has_self = true;
                }
            }
            let enabled_params: Vec<_> = list
                .params()
                .filter(|p| item_enabled(p.syntax(), diagnostics))
                .collect();
            let count = enabled_params.len();
            for (i, param) in enabled_params.into_iter().enumerate() {
                check_inert_attrs(param.attrs(), diagnostics);
                if let Some(dots) = param.dot3_token() {
                    let message = if !in_extern_block {
                        Some("`...` is not supported for non-extern functions")
                    } else if i + 1 != count {
                        Some("`...` must be the last argument of a C-variadic function")
                    } else {
                        None
                    };
                    match message {
                        Some(message) => {
                            diagnostics.push(Diagnostic::new(dots.text_range(), message))
                        }
                        None => is_variadic = true,
                    }
                    continue;
                }
                let name = match param.pat() {
                    Some(ast::Pat::IdentPat(pat)) => pat.name().map(|n| Symbol::intern(n.text())),
                    _ => None,
                };
                let mut ty = lower_type(param.ty(), diagnostics);
                if in_extern_block {
                    let site = ImplTraitSite::NotAllowed("`extern fn` parameters");
                    reject_impl_trait(&mut ty, site, diagnostics);
                } else {
                    desugar_impl_params(&mut ty, &mut impl_params, diagnostics);
                }
                params.push(Param {
                    ty,
                    range: param
                        .ty()
                        .map_or(param.syntax().text_range(), |t| t.syntax().text_range()),
                    whole: param.syntax().text_range(),
                    name,
                });
            }
        }
        if !impl_params.params.is_empty() {
            fn extend<T>(list: &mut Box<[T]>, more: Vec<T>) {
                let mut all = std::mem::take(list).into_vec();
                all.extend(more);
                *list = all.into();
            }
            extend(&mut generics.params, impl_params.params);
            extend(&mut generics.predicates, impl_params.predicates);
            extend(&mut generics.type_outlives, impl_params.type_outlives);
        }
        let (mut ret, ret_range) = match f.ret_type() {
            Some(ret) => (
                lower_type(ret.ty(), diagnostics),
                ret.ty()
                    .map_or(ret.syntax().text_range(), |t| t.syntax().text_range()),
            ),
            None => (TypeRef::unit(), name.syntax().text_range()),
        };
        let mut opaques = Vec::new();
        if in_extern_block {
            let site = ImplTraitSite::NotAllowed("`extern fn` return types");
            reject_impl_trait(&mut ret, site, diagnostics);
        } else {
            collect_opaques(&mut ret, &mut opaques, diagnostics);
            if in_trait_or_trait_impl && !opaques.is_empty() && !matches!(ret, TypeRef::Opaque(_)) {
                diagnostics.push(Diagnostic::new(
                    ret_range,
                    "nested return-position `impl Trait` in trait methods is not supported yet",
                ));
            }
        }
        let has_body = f.body().is_some();
        if matches!(owner, Some(Owner::Impl(_))) && !has_body {
            diagnostics.push(Diagnostic::new(
                TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
                "associated function in `impl` without body",
            ));
        }
        if !has_body
            && !in_extern_block
            && let Some(list) = f.param_list()
        {
            for param in list
                .params()
                .filter(|p| item_enabled(p.syntax(), &mut Vec::new()))
            {
                if let Some(pat) = param.pat()
                    && !matches!(pat, ast::Pat::IdentPat(_) | ast::Pat::WildcardPat(_))
                {
                    diagnostics.push(Diagnostic::new(
                        pat.syntax().text_range(),
                        "patterns aren't allowed in functions without bodies",
                    ));
                }
            }
        }
        if in_extern_block && has_body {
            diagnostics.push(Diagnostic::new(
                name.syntax().text_range(),
                "functions in `extern` blocks cannot have bodies",
            ));
        } else if !in_extern_block && !has_body && owner.is_none() {
            diagnostics.push(Diagnostic::new(
                name.syntax().text_range(),
                "free function without a body",
            ));
        }
        let signature_end = [
            f.ret_type().map(|r| r.syntax()),
            f.param_list().map(|p| p.syntax()),
            Some(name.syntax()),
        ]
        .into_iter()
        .flatten()
        .next()
        .map_or(0, |n| n.text_range().end());
        let sig_end = if has_body {
            f.where_clause()
                .map_or(signature_end, |w| w.syntax().text_range().end())
        } else {
            f.syntax().text_range().end()
        };
        if legacy_const_generic.is_some_and(|position| {
            position as usize != params.len()
                || generics.params.len() != 1
                || !generics.params[0].is_const()
                || owner.is_some()
        }) {
            unsupported(
                &mut self.items.diagnostics,
                f.syntax().text_range(),
                "legacy const generics without one trailing const parameter",
            );
            legacy_const_generic = None;
        }
        let intrinsic = if intrinsic {
            let identity = crate::Intrinsic::from_name(name.text());
            if identity.is_none() {
                self.items.diagnostics.push(Diagnostic::new(
                    name.syntax().text_range(),
                    "unknown compiler intrinsic",
                ));
            }
            identity
        } else {
            None
        };
        let id = self.items.functions.alloc(Function {
            def_range: TextRange::new(def_start(f.syntax()), signature_end),
            sig_range: TextRange::new(def_start(f.syntax()), sig_end),
            item_range: TextRange::new(def_start(f.syntax()), f.syntax().text_range().end()),
            param_list: f
                .param_list()
                .map_or(name.syntax().text_range(), |p| p.syntax().text_range()),
            name: Symbol::intern(name.text()),
            generics,
            params: params.into(),
            ret,
            ret_range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            is_unsafe: in_extern_block || f.unsafe_token().is_some(),
            target_features: has_target_feature.then(|| enabled_features.into_boxed_slice()),
            is_extern: in_extern_block,
            link_name,
            abi,
            is_const: f.const_token().is_some(),
            inherits_overflow_checks,
            const_unstable,
            precise_live_drops,
            intrinsic,
            legacy_const_generic,
            track_caller,
            inline,
            cold,
            do_not_const_check,
            is_variadic,
            owner,
            has_self,
            has_body: has_body && !in_extern_block,
            node: f.syntax().id(),
            name_range: name.syntax().text_range(),
            derived: None,
            ctor: None,
            opaques: opaques.into(),
            macro_scope: self.macro_scope,
        });
        if let Some(metadata) = sized_dispatch {
            if intrinsic == Some(crate::Intrinsic::SizedDispatch) {
                self.items.sized_dispatch_declarations.insert(id, metadata);
            } else {
                self.items.diagnostics.push(Diagnostic::new(
                    f.syntax().text_range(),
                    "sized dispatch metadata requires the sized dispatch intrinsic",
                ));
            }
        }
        self.fn_ids.insert((ctx.file, f.syntax().id()), id);
        if lang.is_some_and(|l| l.as_str() == "panic") {
            self.items.panic_fn = Some(id);
        } else if let Some(lang) = lang {
            self.items.lang_fns.insert(lang, id);
        }
        if let Some(body) = f.body() {
            self.collect_body_items(ctx, body.syntax().id(), Some(id));
        }
        Some(id)
    }

    /// The items declared in the blocks of a body, at `node` in
    /// `ctx.file`, of `function` if it is one's: each block with items
    /// becomes a module inside `ctx.module`.
    fn collect_body_items(&mut self, ctx: Ctx, node: NodeId, function: Option<FunctionId>) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        // Most bodies have no items: they are found without a walk.
        if !tree.node(node).descendants().any(|n| {
            is_item_kind(n.kind())
                || matches!(n.kind(), SyntaxKind::MACRO_EXPR | SyntaxKind::CALL_EXPR)
        }) {
            return;
        }
        self.collect_body_node(ctx, tree.node(node), function);
    }

    /// Block `node`'s items, in a module of their own, then those of the
    /// blocks inside it. A `macro_rules!` statement defines a macro for
    /// the items and blocks after it.
    fn collect_block(&mut self, ctx: Ctx, node: NodeId, function: Option<FunctionId>) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let block = tree.node(node);
        let has_items = block.children().any(|c| {
            is_item_kind(c.kind())
                || c.kind() == SyntaxKind::MACRO_EXPR
                || c.children()
                    .any(|m| m.kind() == SyntaxKind::MACRO_EXPR && !macros::is_macro_rules(m))
        });
        let ctx = if has_items {
            let parent = &self.items.modules[ctx.module];
            let outer = function.map_or(kw::Empty, |f| self.items.functions[f].name);
            let count = self.block_counts.entry(function).or_default();
            let index = *count;
            *count += 1;
            let module = self.items.modules.alloc(Module {
                name: Symbol::intern(&format!("{outer}.{index}")),
                parent: Some(ctx.module),
                krate: parent.krate,
                file: ctx.file,
                dir: parent.dir.clone(),
                vis: Visibility::Module(ctx.module),
                def_range: TextRange::empty(0),
                block: Some(Block { function, node }),
            });
            self.items.block_modules.insert((ctx.file, node), module);
            Ctx {
                module,
                file: ctx.file,
            }
        } else {
            ctx
        };
        self.precollect_imports(ctx, block.children().map(|child| child.id()));
        let outer_scope = self.macro_scope;
        for child in block.children() {
            self.collect_body_node(ctx, child, function);
        }
        self.macro_scope = outer_scope;
    }

    /// Walk expressions without crossing item or block scopes. Retained macro
    /// expansions collect items through the same path as written block items.
    fn collect_body_node(&mut self, ctx: Ctx, node: SyntaxNode<'_>, function: Option<FunctionId>) {
        if is_item_kind(node.kind()) {
            let _file = TypesFile::enter(ctx.file);
            let start = self.items.diagnostics.len();
            if item_enabled(node, &mut self.items.diagnostics) {
                self.collect_item_expanding_types(ctx, node.id());
                self.collect_anon_consts(ctx, node.id());
            }
            self.tag(start, ctx.file);
            return;
        }
        if !item_enabled(node, &mut Vec::new()) {
            return;
        }
        if node.kind() == SyntaxKind::BLOCK_EXPR {
            self.collect_block(ctx, node.id(), function);
            return;
        }
        if node.kind() == SyntaxKind::CALL_EXPR
            && let Some(function) = function
            && let Some(call) = ast::CallExpr::cast(node)
            && matches!(call.expr(), Some(ast::Expr::PathExpr(_)))
            && call
                .arg_list()
                .is_some_and(|args| args.args().any(|arg| is_anon_const(const_expr_root(arg))))
        {
            self.legacy_calls.push(LegacyCall {
                ctx,
                node: node.id(),
                function,
                macro_scope: self.macro_scope,
            });
        }
        if node.kind() == SyntaxKind::MACRO_EXPR {
            if macros::is_macro_rules(node) {
                let start = self.items.diagnostics.len();
                let previous = self.macro_scope;
                self.define_macro(ctx, node, false, false);
                if self.macro_scope != previous
                    && self.items.diagnostics.len() == start
                    && let Some(scope) = self.macro_scope
                {
                    self.items
                        .body_macro_definitions
                        .insert((ctx.file, node.id()), self.items.macro_scopes[scope].def);
                }
                self.items.diagnostics.truncate(start);
                return;
            }
            if self.expansion_depth as usize >= self.items.macro_recursion_limit(ctx.module) {
                return;
            }
            let Some(path) = node.children().find_map(ast::Path::cast) else {
                return;
            };
            let Ok(macros::Resolved::Def(id)) =
                macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module)
            else {
                self.pending_body_macros.push(PendingBodyMacro {
                    ctx,
                    node: node.id(),
                    function,
                    scope: self.macro_scope,
                    depth: self.expansion_depth,
                });
                return;
            };
            let def = self.items.macros[id].clone();
            let fragment = if node.parent().is_some_and(|p| {
                matches!(p.kind(), SyntaxKind::EXPR_STMT)
                    || (matches!(p.kind(), SyntaxKind::BLOCK_EXPR | SyntaxKind::MACRO_STMTS)
                        && !node.children().any(|c| c.kind() == SyntaxKind::ATTR))
            }) {
                flux_syntax::parser::Fragment::Stmts
            } else {
                flux_syntax::parser::Fragment::Expr
            };
            let real = self.real_file(ctx.file);
            let site = macros::CallSite {
                read_text: crate::source::SourceDb::read_include_str,
                scope: self.macro_scope,
                items: self.items,
                krate: self.items.modules[ctx.module].krate,
                module: ctx.module,
                real_file: real,
            };
            let mut diagnostics = Vec::new();
            let Some(expansion) = macros::expand(&def, node, &site, fragment, &mut diagnostics)
            else {
                return;
            };
            // Unsuccessful or diagnostic-bearing expansions are lowered normally,
            // where their errors are reported once with the body context.
            if !diagnostics.is_empty() {
                return;
            }
            self.record_macro_path(ctx, path, id);
            let file = if let Some(path) = expansion.included_path {
                match self
                    .db
                    .add_inclusion(expansion.parse, path, self.db.file(ctx.file).edition)
                {
                    Ok(file) => file,
                    Err(message) => {
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(node.text_range(), message));
                        return;
                    }
                }
            } else {
                let file = self.db.add_expansion(expansion.parse, real);
                self.items.expansion_files.insert(file, real);
                file
            };
            self.items.body_expansions.insert(
                (ctx.file, node.id()),
                BodyExpansion {
                    definition: id,
                    file,
                    ctxts: expansion.ctxts,
                    fragment,
                    anon_consts: false,
                },
            );
            if let Some(function) = function {
                self.body_expansion_consts.push(BodyExpansionConsts {
                    ctx: Ctx { file, ..ctx },
                    call: (ctx.file, node.id()),
                    function,
                });
            }
            let shared = self.db.shared(file);
            self.items.insert_file(file, Arc::clone(&shared));
            self.expansion_depth += 1;
            self.collect_body_node(Ctx { file, ..ctx }, shared.parse.tree().root(), function);
            self.expansion_depth -= 1;
            return;
        }
        for child in node.children() {
            self.collect_body_node(ctx, child, function);
        }
    }

    /// The anonymous constants of retained body expansions, in the scope
    /// of their function as [`Self::anon_walk`] would give it, numbered
    /// after the function's own.
    fn collect_body_expansion_consts(&mut self) {
        if self.body_expansion_consts.is_empty() {
            return;
        }
        let mut counts = rustc_hash::FxHashMap::default();
        for (_, konst) in self.items.consts.iter() {
            if let Some(anon) = konst.anon {
                let count = counts.entry(anon.item).or_insert(0);
                *count = (*count).max(anon.index + 1);
            }
        }
        for pending in std::mem::take(&mut self.body_expansion_consts) {
            let function = &self.items.functions[pending.function];
            let owner_generics = self.items.owner_generics(function.owner);
            let generic_self = matches!(function.owner, Some(Owner::Trait(_)))
                || owner_generics.is_some_and(|g| !g.params.is_empty() || !g.lifetimes.is_empty());
            let generics = owner_generics
                .into_iter()
                .chain(Some(&function.generics))
                .flat_map(|g| g.params.iter().map(|p| (p.name, p.is_const())))
                .collect();
            let mut scope = AnonScope {
                self_adt: None,
                module: pending.ctx.module,
                owner: function.owner.filter(|_| !generic_self),
                generics,
                generic_self,
                function: Some(pending.function),
                item: function.name,
            };
            let shared = self.db.shared(pending.ctx.file);
            let start = self.items.diagnostics.len();
            self.anon_walk(
                pending.ctx.file,
                shared.parse.tree().root(),
                &mut scope,
                &mut counts,
            );
            self.tag(start, pending.ctx.file);
            if let Some(expansion) = self.items.body_expansions.get_mut(&pending.call) {
                expansion.anon_consts = true;
            }
        }
    }

    /// Legacy intrinsic arguments use the existing checked constant-expression
    /// pipeline, including retained body-macro expansions. Runtime arguments
    /// and explicit turbofish calls never allocate a legacy constant.
    fn collect_legacy_consts(&mut self) {
        use crate::def_map::{Def, Ns, Resolved};
        if self.legacy_calls.is_empty() {
            return;
        }
        let mut counts = rustc_hash::FxHashMap::default();
        for (_, konst) in self.items.consts.iter() {
            if let Some(anon) = konst.anon {
                let count = counts.entry(anon.item).or_insert(0);
                *count = (*count).max(anon.index + 1);
            }
        }
        for pending in std::mem::take(&mut self.legacy_calls) {
            let shared = self.db.shared(pending.ctx.file);
            let node = shared.parse.tree().node(pending.node);
            let Some(call) = ast::CallExpr::cast(node) else {
                continue;
            };
            let Some(ast::Expr::PathExpr(callee)) = call.expr() else {
                continue;
            };
            let Some(path) = callee.path().and_then(|p| lower_path(p, &mut Vec::new())) else {
                continue;
            };
            let Ok(Resolved::Def(Def::Function(callee))) =
                self.items.resolve(pending.ctx.module, &path, Ns::Values)
            else {
                continue;
            };
            let callee = &self.items.functions[callee];
            let Some(position) = callee.legacy_const_generic else {
                continue;
            };
            let function = &self.items.functions[pending.function];
            if let Some(name) = path.as_name() {
                let mut file = pending.ctx.file;
                let mut at = node.text_range().start();
                let mut shadowed = false;
                loop {
                    let tree = self.db.shared(file);
                    let mut locals = Vec::new();
                    let scope = if file == function.loc.file {
                        tree.parse.tree().node(function.node)
                    } else {
                        tree.parse.tree().root()
                    };
                    crate::lower::visible_bindings(scope, at, &mut locals);
                    if locals.iter().any(|local| *local == name.as_str()) {
                        shadowed = true;
                        break;
                    }
                    if file == function.loc.file {
                        break;
                    }
                    let Some((&(parent, call), _)) = self
                        .items
                        .body_expansions
                        .iter()
                        .find(|(_, expansion)| expansion.file == file)
                    else {
                        break;
                    };
                    file = parent;
                    at = self
                        .db
                        .shared(parent)
                        .parse
                        .tree()
                        .node(call)
                        .text_range()
                        .start();
                }
                if shadowed {
                    continue;
                }
            }
            let args: Vec<_> = call
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .filter(|arg| item_enabled(arg.syntax(), &mut Vec::new()))
                .collect();
            if args.len() != callee.params.len() + 1 {
                continue;
            }
            let expr = const_expr_root(args[position as usize]);
            if !is_anon_const(expr)
                || self
                    .items
                    .anon_consts
                    .contains_key(&(pending.ctx.file, expr.syntax().id()))
            {
                continue;
            }
            // Keep the existing direct representation of negative integer
            // literals, which type-only checking can evaluate without MIR.
            if let ast::Expr::PrefixExpr(prefix) = expr
                && prefix.op_kind() == Some(ast::UnaryOp::Neg)
                && let Some(ast::Expr::Literal(literal)) = prefix.expr().map(const_expr_root)
                && literal.kind() == Some(ast::LiteralKind::Int)
            {
                continue;
            }
            let owner_generics = self.items.owner_generics(function.owner);
            let generics = owner_generics
                .into_iter()
                .chain(Some(&function.generics))
                .flat_map(|g| g.params.iter().map(|p| (p.name, p.is_const())))
                .collect();
            let scope = AnonScope {
                self_adt: None,
                module: pending.ctx.module,
                owner: function.owner,
                generics,
                generic_self: function
                    .owner
                    .is_some_and(|owner| matches!(owner, Owner::Trait(_)))
                    || owner_generics
                        .is_some_and(|g| !g.params.is_empty() || !g.lifetimes.is_empty()),
                function: Some(pending.function),
                item: function.name,
            };
            let start = self.items.diagnostics.len();
            let old_scope = self.macro_scope;
            self.macro_scope = pending.macro_scope;
            self.anon_const(pending.ctx.file, expr, AnonUse::Arg, &scope, &mut counts);
            self.macro_scope = old_scope;
            self.tag(start, pending.ctx.file);
        }
    }

    /// Allocates the anonymous constants of item `node` (array lengths,
    /// repeat counts and const generic arguments written as expressions
    /// other than literals and paths), its associated items' and its
    /// body's, but not those of items inside it, which are collected on
    /// their own. Uses of generic parameters in them are rustc's errors,
    /// and they get none.
    fn collect_anon_consts(&mut self, ctx: Ctx, node: NodeId) {
        let shared = self.db.shared(ctx.file);
        let tree = shared.parse.tree();
        let root = tree.node(node);
        // Most items have none: they are found without a walk.
        if !root.descendants().any(|n| {
            matches!(
                n.kind(),
                SyntaxKind::ARRAY_TYPE | SyntaxKind::ARRAY_EXPR | SyntaxKind::CONST_ARG
            )
        }) {
            return;
        }
        let mut scope = AnonScope {
            self_adt: None,
            module: ctx.module,
            owner: None,
            generics: Vec::new(),
            generic_self: false,
            function: None,
            item: kw::Empty,
        };
        let mut counts = rustc_hash::FxHashMap::default();
        self.anon_walk(ctx.file, root, &mut scope, &mut counts);
    }

    fn anon_walk(
        &mut self,
        file: FileId,
        node: SyntaxNode<'_>,
        scope: &mut AnonScope,
        counts: &mut rustc_hash::FxHashMap<Symbol, u32>,
    ) {
        let kind = node.kind();
        if !item_enabled(node, &mut Vec::new()) {
            return;
        }
        let outer = scope.clone();
        if is_item_kind(kind) {
            // Named nested items do not inherit an outer declaration's Self.
            scope.self_adt = self.adt_ids.get(&(file, node.id())).copied();
            if scope.self_adt.is_some() {
                scope.owner = None;
                scope.generic_self = false;
            }
            if let Some(list) = node
                .children()
                .find(|c| c.kind() == SyntaxKind::GENERIC_PARAM_LIST)
                .and_then(ast::GenericParamList::cast)
            {
                for param in list.generic_params() {
                    let (name, is_const) = match param {
                        ast::GenericParam::TypeParam(p) => (p.name(), false),
                        ast::GenericParam::ConstParam(p) => (p.name(), true),
                        ast::GenericParam::LifetimeParam(_) => {
                            if kind == SyntaxKind::IMPL || scope.self_adt.is_some() {
                                scope.generic_self = true;
                            }
                            continue;
                        }
                    };
                    if let Some(name) = name {
                        scope.generics.push((Symbol::intern(name.text()), is_const));
                        if matches!(kind, SyntaxKind::IMPL | SyntaxKind::TRAIT)
                            || scope.self_adt.is_some()
                        {
                            scope.generic_self = true;
                        }
                    }
                }
            }
            if let Some(name) = node
                .children()
                .find(|c| c.kind() == SyntaxKind::NAME)
                .map(|n| Symbol::intern(n.text().trim()))
                && scope.item == kw::Empty
            {
                scope.item = name;
            }
            match kind {
                SyntaxKind::FN => {
                    scope.function = self.fn_ids.get(&(file, node.id())).copied();
                    if let Some(f) = scope.function {
                        scope.item = self.items.functions[f].name;
                    }
                }
                SyntaxKind::IMPL => {
                    scope.owner = self
                        .impl_ids
                        .get(&(file, node.id()))
                        .filter(|_| !scope.generic_self)
                        .map(|&imp| Owner::Impl(imp));
                }
                SyntaxKind::TRAIT => scope.generic_self = true,
                _ => {}
            }
        }
        match kind {
            SyntaxKind::BLOCK_EXPR => {
                if let Some(&module) = self.items.block_modules.get(&(file, node.id())) {
                    scope.module = module;
                }
            }
            SyntaxKind::ARRAY_TYPE => {
                if let Some(expr) = ast::ArrayType::cast(node).and_then(|a| a.expr()) {
                    self.anon_const(file, expr, AnonUse::Len, scope, counts);
                }
            }
            SyntaxKind::ARRAY_EXPR => {
                if let Some(array) = ast::ArrayExpr::cast(node)
                    && array.semicolon_token().is_some()
                    && let Some(expr) = array.exprs().nth(1)
                {
                    self.anon_const(file, expr, AnonUse::Repeat, scope, counts);
                }
            }
            SyntaxKind::CONST_ARG => {
                if let Some(expr) = ast::ConstArg::cast(node).and_then(|a| a.expr()) {
                    self.anon_const(file, expr, AnonUse::Arg, scope, counts);
                }
            }
            _ => {}
        }
        // Associated items and those of `extern` blocks are this item's;
        // others are collected on their own.
        let own_items = matches!(
            kind,
            SyntaxKind::ASSOC_ITEM_LIST | SyntaxKind::EXTERN_ITEM_LIST
        );
        for child in node.children() {
            if is_item_kind(child.kind()) && !own_items {
                continue;
            }
            self.anon_walk(file, child, scope, counts);
        }
        *scope = outer;
    }

    /// The anonymous constant of `expr`, used as `what`, if it is one
    /// and does not use generic parameters (reported).
    fn anon_const(
        &mut self,
        file: FileId,
        expr: ast::Expr<'_>,
        what: AnonUse,
        scope: &AnonScope,
        counts: &mut rustc_hash::FxHashMap<Symbol, u32>,
    ) {
        let expr = const_expr_root(expr);
        if !is_anon_const(expr) {
            return;
        }
        let range = expr.syntax().text_range();
        let count = counts.entry(scope.item).or_default();
        let index = *count;
        *count += 1;
        let diagnostics = &mut self.items.diagnostics;
        let mut generic = false;
        let mut depends = false;
        for token in expr.syntax().tokens() {
            match token.kind() {
                SyntaxKind::SELF_TYPE_KW if scope.generic_self => {
                    diagnostics.push(Diagnostic::new(
                        token.text_range(),
                        "generic `Self` types are currently not permitted in anonymous constants",
                    ));
                    generic = true;
                }
                SyntaxKind::IDENT => {
                    let Some(&(_, is_const)) = scope
                        .generics
                        .iter()
                        .rev()
                        .find(|(n, _)| n.as_str() == token.text())
                    else {
                        continue;
                    };
                    generic = true;
                    // A repeat count may name type parameters, as rustc
                    // allows for compatibility; its value then depends
                    // on them.
                    if what == AnonUse::Repeat && !is_const {
                        depends = true;
                        continue;
                    }
                    diagnostics.push(Diagnostic::new(
                        token.text_range(),
                        "generic parameters may not be used in const operations",
                    ));
                }
                _ => {}
            }
        }
        if depends {
            diagnostics.push(Diagnostic::new(
                range,
                "constant expression depends on a generic parameter",
            ));
        }
        if generic {
            return;
        }
        let ty = match what {
            AnonUse::Arg => TypeRef::Infer,
            AnonUse::Len | AnonUse::Repeat => TypeRef::Path(Path::single(sym::usize)),
        };
        let konst = self.items.consts.alloc(Const {
            name: None,
            ty,
            ty_range: range,
            loc: ItemLoc {
                module: scope.module,
                file,
                vis: Visibility::Public,
            },
            owner: scope.owner,
            node: expr.syntax().id(),
            has_body: true,
            def_range: range,
            item_range: range,
            discriminant: None,
            is_static: None,
            extern_symbol: None,
            specialization: false,
            anon: Some(AnonConst {
                self_adt: scope.self_adt,
                function: scope.function,
                item: scope.item,
                index,
                is_arg: what == AnonUse::Arg,
            }),
            macro_scope: self.macro_scope,
        });
        self.items
            .anon_consts
            .insert((file, expr.syntax().id()), konst);
    }

    fn collect_const(
        &mut self,
        ctx: Ctx,
        c: ast::Const<'_>,
        owner: Option<Owner>,
    ) -> Option<ConstId> {
        let is_core = self.items.is_core(ctx.module);
        let mut specialization = false;
        let attrs = crate::cfg::Attrs::new(c.attrs());
        for attr in attrs.iter() {
            if is_core && attr_name(attr) == Some("rustc_specialization") {
                specialization = true;
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.item_visibility(ctx, c.visibility(), owner);
        let name = c.name();
        let name_end = name
            .map(|n| n.syntax().text_range().end())
            .or_else(|| c.underscore_token().map(|t| t.text_range().end()))?;
        let diagnostics = &mut self.items.diagnostics;
        let ty = match c.ty() {
            Some(ty) => {
                let mut ty = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::NotAllowed("const types");
                reject_impl_trait(&mut ty, site, diagnostics);
                ty
            }
            None => {
                diagnostics.push(Diagnostic::new(
                    TextRange::empty(name_end),
                    "missing type for `const` item",
                ));
                TypeRef::Error
            }
        };

        let has_body = c.body().is_some();
        let def_start = def_start(c.syntax());
        let header_end = c.ty().map_or(name_end, |t| t.syntax().text_range().end());
        let item_range = TextRange::new(def_start, c.syntax().text_range().end());
        if !has_body {
            let message = match owner {
                Some(Owner::Trait(_)) => None,
                Some(Owner::Impl(_)) => Some("associated constant in `impl` without body"),
                None => Some("free constant item without body"),
            };
            if let Some(message) = message {
                diagnostics.push(Diagnostic::new(item_range, message));
            }
        }
        let id = self.items.consts.alloc(Const {
            name: name.map(|n| Symbol::intern(n.text())),
            ty,
            ty_range: c
                .ty()
                .map_or(TextRange::empty(name_end), |t| t.syntax().text_range()),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            owner,
            node: c.syntax().id(),
            has_body,
            def_range: TextRange::new(def_start, header_end),
            item_range,
            discriminant: None,
            anon: None,
            is_static: None,
            extern_symbol: None,
            specialization,
            macro_scope: self.macro_scope,
        });
        if let Some(body) = c.body() {
            self.collect_body_items(ctx, body.syntax().id(), None);
        }
        Some(id)
    }

    /// `static X: T = v;` or `static mut X: T = v;`.
    fn collect_static(&mut self, ctx: Ctx, st: ast::Static<'_>, foreign: bool) {
        check_attrs(st.attrs(), &mut self.items.diagnostics);
        let vis = self.visibility(ctx, st.visibility());
        let Some(name) = st.name() else {
            return;
        };
        let name_end = name.syntax().text_range().end();
        let diagnostics = &mut self.items.diagnostics;
        let ty = match st.ty() {
            Some(ty) => {
                let mut ty = lower_type(Some(ty), diagnostics);
                let site = ImplTraitSite::NotAllowed("static types");
                reject_impl_trait(&mut ty, site, diagnostics);
                ty
            }
            None => {
                diagnostics.push(Diagnostic::new(
                    TextRange::empty(name_end),
                    "missing type for `static` item",
                ));
                TypeRef::Error
            }
        };
        let has_body = st.body().is_some();
        let def_start = def_start(st.syntax());
        let header_end = st.ty().map_or(name_end, |t| t.syntax().text_range().end());
        let item_range = TextRange::new(def_start, st.syntax().text_range().end());
        if !has_body && !foreign {
            diagnostics.push(Diagnostic::new(item_range, "free static item without body"));
        }
        let mut extern_symbol = foreign.then(|| Symbol::intern(name.text()));
        if foreign {
            for attr in crate::cfg::Attrs::new(st.attrs()).iter() {
                if attr_name(attr) == Some("link_name") {
                    extern_symbol =
                        super::attributes::link_name_attr(attr, diagnostics).or(extern_symbol);
                    break;
                }
            }
            if has_body {
                diagnostics.push(Diagnostic::new(
                    item_range,
                    "foreign statics cannot have initializers",
                ));
            }
            if st
                .syntax()
                .children_with_tokens()
                .any(|e| matches!(e, flux_syntax::SyntaxElement::Token(t) if t.text() == "safe"))
            {
                unsupported(diagnostics, item_range, "safe foreign statics");
            }
        }
        self.items.consts.alloc(Const {
            name: Some(Symbol::intern(name.text())),
            ty,
            ty_range: st
                .ty()
                .map_or(TextRange::empty(name_end), |t| t.syntax().text_range()),
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            owner: None,
            node: st.syntax().id(),
            has_body,
            def_range: TextRange::new(def_start, header_end),
            item_range,
            discriminant: None,
            anon: None,
            is_static: Some(st.mut_token().is_some()),
            extern_symbol,
            specialization: false,
            macro_scope: self.macro_scope,
        });
        if let Some(body) = st.body() {
            self.collect_body_items(ctx, body.syntax().id(), None);
        }
    }

    /// `extern crate alloc;`: `alloc` may be named by path, as rustc's
    /// `alloc` may once declared. `core` always may.
    fn collect_extern_crate(&mut self, ctx: Ctx, e: ast::ExternCrate<'_>) {
        let range = e.syntax().text_range();
        let Some(name) = e.name_ref() else {
            return;
        };
        let name = Symbol::intern(name.text());
        let krate = self.items.modules[ctx.module].krate;
        let alias = match e.rename() {
            Some(rename) => match rename.name() {
                Some(alias) => Symbol::intern(alias.text()),
                None => {
                    unsupported(
                        &mut self.items.diagnostics,
                        range,
                        "`extern crate` underscore aliases",
                    );
                    return;
                }
            },
            None => name,
        };
        if name == kw::SelfLower && e.rename().is_none() {
            self.items.diagnostics.push(Diagnostic::new(
                range,
                "`extern crate self;` requires renaming",
            ));
            return;
        }
        let own = &self.items.crates[krate.0 as usize];
        let target = if name == kw::SelfLower {
            Some(krate)
        } else {
            None
        }
        .or_else(|| {
            own.dependencies
                .iter()
                .find(|(alias, _)| *alias == name)
                .map(|(_, target)| *target)
        })
        .or_else(|| {
            self.items
                .crates
                .iter()
                .enumerate()
                .find(|(i, c)| {
                    (*i as u32) < krate.0 && c.name == name && (c.is_core || name == sym::std)
                })
                .map(|(i, _)| CrateId(i as u32))
        });
        let Some(target) = target else {
            self.items.diagnostics.push(Diagnostic::new(
                range,
                format!("can't find crate for `{name}`"),
            ));
            return;
        };
        if target != krate {
            self.items.load_extern(krate, target);
        }
        let attrs = crate::cfg::Attrs::new(e.attrs());
        for attr in attrs.iter() {
            if attr_name(attr) == Some("macro_use") {
                if attr.meta().is_some_and(|m| {
                    m.token_tree().is_none() && m.eq_token().is_none() && m.unsafe_token().is_none()
                }) {
                    if ctx.module != self.items.crates[krate.0 as usize].root {
                        self.items.diagnostics.push(Diagnostic::new(
                            range,
                            "an extern crate loading macros must be at the crate root",
                        ));
                    } else {
                        let mut exported: Vec<_> = self
                            .items
                            .exported_macros
                            .iter()
                            .filter_map(|(&(owner, name), &id)| {
                                (owner == target).then_some((name, id))
                            })
                            .collect();
                        if let Some(scope) = self
                            .items
                            .def_map
                            .scopes
                            .get(self.items.module_of(target).index())
                        {
                            exported.extend(scope.macros.iter().filter_map(|(&name, binding)| {
                                match binding.def {
                                    crate::def_map::Def::Macro(id)
                                        if binding.vis == Visibility::Public =>
                                    {
                                        Some((name, id))
                                    }
                                    _ => None,
                                }
                            }));
                        }
                        self.items.crates[krate.0 as usize]
                            .macro_use
                            .extend(exported);
                    }
                } else {
                    unsupported(
                        &mut self.items.diagnostics,
                        attr.syntax().text_range(),
                        "selective extern macro imports",
                    );
                }
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let own = &self.items.crates[krate.0 as usize];
        let root = own.root;
        if ctx.module == root && e.rename().is_some() {
            self.items.crates[krate.0 as usize]
                .extern_aliases
                .push((alias, target));
        }
        if ctx.module == root && e.rename().is_none() && (name == sym::alloc || name == sym::std) {
            self.items.crates[krate.0 as usize].externs.push(name);
        }
        let vis = self.visibility(ctx, e.visibility());
        self.items.uses.push(Use {
            module: ctx.module,
            file: ctx.file,
            vis,
            path: Path {
                edition: None,
                anchor: PathAnchor::DefinitionCrate(target),
                qself: None,
                segments: Box::default(),
                ranges: Box::default(),
                anchor_ranges: Box::default(),
                args: Box::default(),
            },
            kind: UseKind::Module(alias),
            range,
        });
    }

    fn is_derive_attr(&self, ctx: Ctx, attr: ast::Attr<'_>) -> bool {
        if attr_name(attr) == Some("derive") {
            return true;
        }
        let Some(path) = attr.meta().and_then(|m| m.path()) else {
            return false;
        };
        let Some(path) = lower_path(path, &mut Vec::new()) else {
            return false;
        };
        matches!(macros::resolve_predecessor(self.items,ctx.module,&path,crate::def_map::Ns::Macros),
            Some(crate::def_map::Resolved::Def(crate::def_map::Def::Macro(id)))
            if matches!(self.items.macros[id].kind,macros::MacroKind::Builtin(macros::Builtin::Derive)))
    }

    fn collect_struct(&mut self, ctx: Ctx, s: ast::Struct<'_>) {
        let mut derives = Vec::new();
        let mut non_exhaustive = false;
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let mut hints = Vec::new();
        let mut repr_align = 1;
        let mut repr_pack = None;
        let attrs = crate::cfg::Attrs::new(s.attrs());
        for attr in attrs.iter() {
            if self.is_derive_attr(ctx, attr) {
                let mut diagnostics = Vec::new();
                derive_attr(attr, &mut derives, &mut diagnostics, self.items, ctx.module);
                self.items.diagnostics.extend(diagnostics);
            } else if attr_name(attr) == Some("repr") {
                repr_attr(
                    attr,
                    &mut hints,
                    &mut repr_align,
                    &mut repr_pack,
                    &mut self.items.diagnostics,
                );
            } else if attr_name(attr) == Some("non_exhaustive") {
                non_exhaustive |= non_exhaustive_attr(attr, &mut self.items.diagnostics);
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.visibility(ctx, s.visibility());
        let Some(name) = s.name() else {
            return;
        };
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(
            s.generic_param_list(),
            s.where_clause(),
            Defaults::Struct,
            diagnostics,
        );
        let (kind, fields) = self.collect_fields(
            ctx,
            s.record_field_list(),
            s.tuple_field_list(),
            false,
            false,
        );
        let repr_c = struct_repr(&hints, &mut self.items.diagnostics);
        if repr_pack.is_some() && hints.iter().any(|(h, _)| h.as_str() == "align") {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "packed structs cannot have an alignment hint",
            ));
        }
        let repr_transparent = hints.iter().any(|(hint, _)| hint.as_str() == "transparent");
        if repr_transparent && hints.iter().any(|(hint, _)| hint.as_str() != "transparent") {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "transparent structs cannot have other repr hints",
            ));
        }
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            is_union: false,
            non_exhaustive,
            name: Symbol::intern(name.text()),
            generics,
            kind,
            fields: fields.into(),
            variants: None,
            repr: None,
            repr_c,
            repr_transparent,
            repr_align,
            repr_pack,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(s.syntax()), name_range.end()),
        });
        if let Some(lang) = lang {
            self.items.lang_enums.insert(lang, id);
        }
        for (derive, range) in derives {
            self.derives.push((ctx, id, derive, range, s.syntax().id()));
        }
        self.adt_ids.insert((ctx.file, s.syntax().id()), id);
        self.ctors.push((ctx, id, s.syntax().id()));
    }

    fn collect_union(&mut self, ctx: Ctx, s: ast::Union<'_>) {
        let mut derives = Vec::new();
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let mut hints = Vec::new();
        let mut repr_align = 1;
        let mut repr_pack = None;
        let attrs = crate::cfg::Attrs::new(s.attrs());
        for attr in attrs.iter() {
            if self.is_derive_attr(ctx, attr) {
                let mut diagnostics = Vec::new();
                derive_attr(attr, &mut derives, &mut diagnostics, self.items, ctx.module);
                self.items.diagnostics.extend(diagnostics);
            } else if attr_name(attr) == Some("repr") {
                repr_attr(
                    attr,
                    &mut hints,
                    &mut repr_align,
                    &mut repr_pack,
                    &mut self.items.diagnostics,
                );
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.visibility(ctx, s.visibility());
        let Some(name) = s.name() else {
            return;
        };
        if s.generic_param_list().is_some() || s.where_clause().is_some() {
            unsupported(
                &mut self.items.diagnostics,
                s.syntax().text_range(),
                "generic unions",
            );
        }
        for (derive, range) in &derives {
            if !matches!(derive, Derive::Copy | Derive::Clone) {
                unsupported(&mut self.items.diagnostics, *range, "this union derive");
            }
        }
        if derives.iter().any(|(d, _)| *d == Derive::Clone)
            && !derives.iter().any(|(d, _)| *d == Derive::Copy)
        {
            unsupported(
                &mut self.items.diagnostics,
                s.syntax().text_range(),
                "union Clone without Copy",
            );
        }
        let diagnostics = &mut self.items.diagnostics;
        let generics = collect_generics(
            s.generic_param_list(),
            s.where_clause(),
            Defaults::Struct,
            diagnostics,
        );
        let (kind, fields) = self.collect_fields(ctx, s.record_field_list(), None, false, true);
        let repr_c = struct_repr(&hints, &mut self.items.diagnostics);
        if hints.iter().any(|(hint, _)| hint.as_str() == "transparent") {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "transparent unions are not supported",
            ));
        }
        if repr_c && hints.iter().any(|(h, _)| h.as_str() == "Rust") {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "conflicting representation hints",
            ));
        }
        if fields.is_empty() {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "unions cannot have zero fields",
            ));
        }
        if repr_pack.is_some() && hints.iter().any(|(hint, _)| hint.as_str() == "align") {
            self.items.diagnostics.push(Diagnostic::new(
                s.syntax().text_range(),
                "packed unions cannot have an alignment hint",
            ));
        }
        if repr_align != 1 {
            unsupported(
                &mut self.items.diagnostics,
                s.syntax().text_range(),
                "union alignment hints",
            );
        }
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            is_union: true,
            non_exhaustive: false,
            name: Symbol::intern(name.text()),
            generics,
            kind,
            fields: fields.into(),
            variants: None,
            repr: None,
            repr_c,
            repr_transparent: false,
            repr_align,
            repr_pack,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range: TextRange::new(def_start(s.syntax()), name_range.end()),
        });
        if let Some(lang) = lang {
            self.items.lang_enums.insert(lang, id);
        }
        for (derive, range) in derives {
            self.derives.push((ctx, id, derive, range, s.syntax().id()));
        }
        self.adt_ids.insert((ctx.file, s.syntax().id()), id);
        self.ctors.push((ctx, id, s.syntax().id()));
    }

    /// The fields of a struct or variant, and so its kind. A variant's
    /// fields are as visible as its enum, and may not say otherwise.
    fn collect_fields(
        &mut self,
        ctx: Ctx,
        record: Option<ast::RecordFieldList<'_>>,
        tuple: Option<ast::TupleFieldList<'_>>,
        in_variant: bool,
        in_union: bool,
    ) -> (StructKind, Vec<Field>) {
        let mut fields = Vec::new();
        let field_vis = |this: &mut Self, vis: Option<ast::Visibility<'_>>| {
            if !in_variant {
                return this.visibility(ctx, vis);
            }
            if let Some(vis) = vis {
                this.items.diagnostics.push(Diagnostic::new(
                    vis.syntax().text_range(),
                    "visibility qualifiers are not permitted here",
                ));
            }
            Visibility::Public
        };
        let kind = if let Some(list) = record {
            for field in list.fields() {
                if !item_enabled(field.syntax(), &mut self.items.diagnostics) {
                    continue;
                }
                if in_union {
                    let attrs = crate::cfg::Attrs::new(field.attrs());
                    for attr in attrs.iter().filter(|a| attr_name(*a) != Some("cfg")) {
                        check_attr(attr, &mut self.items.diagnostics);
                    }
                } else {
                    check_inert_attrs(field.attrs(), &mut self.items.diagnostics);
                }
                let vis = field_vis(self, field.visibility());
                let diagnostics = &mut self.items.diagnostics;
                let Some(field_name) = field.name() else {
                    continue;
                };
                let field_name_range = field_name.syntax().text_range();
                let field_name = Symbol::intern(field_name.text());
                let def_range =
                    TextRange::new(def_start(field.syntax()), field.syntax().text_range().end());
                if fields.iter().any(|f: &Field| f.name == field_name) {
                    diagnostics.push(Diagnostic::new(
                        def_range,
                        format!("field `{field_name}` is already declared"),
                    ));
                    continue;
                }
                let mut ty = lower_type(field.ty(), diagnostics);
                reject_impl_trait(&mut ty, FIELD_TYPES, diagnostics);
                fields.push(Field {
                    name: field_name,
                    ty,
                    vis,
                    range: field
                        .ty()
                        .map_or(field_name_range, |t| t.syntax().text_range()),
                    name_range: field_name_range,
                    def_range,
                });
            }
            StructKind::Record
        } else if let Some(list) = tuple {
            for field in list.fields() {
                if !item_enabled(field.syntax(), &mut self.items.diagnostics) {
                    continue;
                }
                let i = fields.len();
                check_inert_attrs(field.attrs(), &mut self.items.diagnostics);
                let vis = field_vis(self, field.visibility());
                let range = field.syntax().text_range();
                let diagnostics = &mut self.items.diagnostics;
                let mut ty = lower_type(field.ty(), diagnostics);
                reject_impl_trait(&mut ty, FIELD_TYPES, diagnostics);
                fields.push(Field {
                    name: Symbol::intern(&i.to_string()),
                    ty,
                    vis,
                    range: field.ty().map_or(range, |t| t.syntax().text_range()),
                    name_range: range,
                    def_range: range,
                });
            }
            StructKind::Tuple
        } else {
            StructKind::Unit
        };
        (kind, fields)
    }

    fn collect_enum(&mut self, ctx: Ctx, e: ast::Enum<'_>) {
        let mut derives = Vec::new();
        let mut non_exhaustive = false;
        let mut hints = Vec::new();
        let mut repr_align = 1;
        let mut repr_pack = None;
        let is_core = self.items.is_core(ctx.module);
        let mut lang = None;
        let attrs = crate::cfg::Attrs::new(e.attrs());
        for attr in attrs.iter() {
            if self.is_derive_attr(ctx, attr) {
                let mut diagnostics = Vec::new();
                derive_attr(attr, &mut derives, &mut diagnostics, self.items, ctx.module);
                self.items.diagnostics.extend(diagnostics);
            } else if attr_name(attr) == Some("repr") {
                repr_attr(
                    attr,
                    &mut hints,
                    &mut repr_align,
                    &mut repr_pack,
                    &mut self.items.diagnostics,
                );
            } else if attr_name(attr) == Some("non_exhaustive") {
                non_exhaustive |= non_exhaustive_attr(attr, &mut self.items.diagnostics);
            } else if is_core && let Some(name) = lang_attr(attr) {
                lang = Some(name);
            } else {
                check_attr(attr, &mut self.items.diagnostics);
            }
        }
        let vis = self.visibility(ctx, e.visibility());
        let Some(name) = e.name() else {
            return;
        };
        let generics = collect_generics(
            e.generic_param_list(),
            e.where_clause(),
            Defaults::Struct,
            &mut self.items.diagnostics,
        );
        let def_range = TextRange::new(def_start(e.syntax()), name.syntax().text_range().end());
        let enabled_variants: Vec<_> = e
            .variant_list()
            .into_iter()
            .flat_map(|l| l.variants())
            .filter(|v| item_enabled(v.syntax(), &mut self.items.diagnostics))
            .collect();
        let all_variants = enabled_variants.iter().copied();
        let has_fields = all_variants
            .clone()
            .any(|v| v.record_field_list().is_some() || v.tuple_field_list().is_some());
        let repr = enum_repr(
            &hints,
            has_fields,
            all_variants.clone().next().is_none(),
            &mut self.items.diagnostics,
        );
        let repr_name = repr.map_or("isize", |r| r.as_str());
        let values = int_range(repr_name);
        // Whether a discriminant is an expression other than a literal,
        // whose value is known only once constants are evaluated: then
        // duplicates are found after evaluation.
        let computed = all_variants
            .clone()
            .any(|v| v.expr().is_some_and(|e| discriminant_value(e).is_none()));
        let strukt = StructId::from_raw(self.items.structs.len() as u32);
        let generic_names: Vec<Symbol> = generics.params.iter().map(|p| p.name).collect();
        let mut variants: Vec<Variant> = Vec::new();
        // The next discriminant, `None` past the type's maximum.
        let mut next = Some(Discriminant::Value(0));
        // Whether a discriminant is written, which rustc allows on an
        // enum with fields only with an integer `repr`.
        let mut written = false;
        let mut duplicates = Vec::new();
        // `#[default]` on variants: the variant's index, the attribute's
        // range and whether it has a value, and the name's range.
        let mut defaults: Vec<(u32, TextRange, bool, TextRange)> = Vec::new();
        for v in enabled_variants {
            let mut variant_non_exhaustive = false;
            let mut marked = Vec::new();
            let attrs = crate::cfg::Attrs::new(v.attrs());
            self.items.diagnostics.extend(
                attrs
                    .errors
                    .iter()
                    .map(|(r, m)| Diagnostic::new(*r, m.clone())),
            );
            for attr in attrs.iter() {
                if attr_name(attr) == Some("default") {
                    let meta = attr.meta();
                    let has_value = meta.is_some_and(|m| {
                        m.token_tree().is_some()
                            || m.syntax()
                                .children_with_tokens()
                                .any(|t| t.kind() == SyntaxKind::EQ)
                    });
                    let name = meta
                        .and_then(|m| m.path())
                        .map_or(attr.syntax().text_range(), |p| p.syntax().text_range());
                    marked.push((attr.syntax().text_range(), has_value, name));
                } else if attr_name(attr) == Some("non_exhaustive") {
                    variant_non_exhaustive |=
                        non_exhaustive_attr(attr, &mut self.items.diagnostics);
                } else {
                    check_attr(attr, &mut self.items.diagnostics);
                }
            }
            if let Some(vis) = v.visibility() {
                self.items.diagnostics.push(Diagnostic::new(
                    vis.syntax().text_range(),
                    "visibility qualifiers are not permitted here",
                ));
            }
            let Some(v_name) = v.name() else {
                continue;
            };
            let (kind, fields) = self.collect_fields(
                ctx,
                v.record_field_list(),
                v.tuple_field_list(),
                true,
                false,
            );
            let name_range = v_name.syntax().text_range();
            let v_name = Symbol::intern(v_name.text());
            let v_range = TextRange::new(def_start(v.syntax()), v.syntax().text_range().end());
            let diagnostics = &mut self.items.diagnostics;
            let placeholder = next.unwrap_or(Discriminant::Value(0));
            // An expression, which becomes an anonymous constant.
            let mut anonymous = None;
            let mut discriminant = match v.expr() {
                Some(expr) => {
                    written = true;
                    let range = expr.syntax().text_range();
                    match discriminant_value(expr) {
                        Some((_, _, Some((suffix, at)))) if suffix.as_str() != repr_name => {
                            diagnostics.push(Diagnostic::new(at, "mismatched types"));
                            placeholder
                        }
                        Some((true, _, _)) if repr_name.starts_with('u') => {
                            diagnostics.push(Diagnostic::new(
                                range,
                                format!("cannot apply unary operator `-` to type `{repr_name}`"),
                            ));
                            placeholder
                        }
                        Some((negative, value, _)) => {
                            let value = if negative {
                                0i128.checked_sub_unsigned(value)
                            } else {
                                i128::try_from(value).ok()
                            };
                            Discriminant::Value(match value {
                                Some(value) if values.contains(&value) => value,
                                None if repr_name == "u128" && !negative => {
                                    unsupported(
                                        diagnostics,
                                        range,
                                        "discriminants above `i128::MAX`",
                                    );
                                    0
                                }
                                _ => {
                                    diagnostics.push(Diagnostic::new(
                                        range,
                                        format!("literal out of range for `{repr_name}`"),
                                    ));
                                    0
                                }
                            })
                        }
                        None => {
                            if !generic_in_discriminant(expr, &generic_names, diagnostics) {
                                anonymous = Some(range);
                            }
                            placeholder
                        }
                    }
                }
                None => match next {
                    Some(value) => value,
                    None => {
                        diagnostics
                            .push(Diagnostic::new(name_range, "enum discriminant overflowed"));
                        // As rustc, wrapped around.
                        Discriminant::Value(*values.start())
                    }
                },
            };
            if !computed
                && variants.iter().any(|p| p.discriminant == discriminant)
                && !duplicates.contains(&discriminant)
                && let Discriminant::Value(value) = discriminant
            {
                duplicates.push(discriminant);
                diagnostics.push(Diagnostic::new(
                    def_range,
                    format!("discriminant value `{value}` assigned more than once"),
                ));
            }
            if variants.iter().any(|p| p.name == v_name) {
                diagnostics.push(Diagnostic::new(
                    name_range,
                    format!("the name `{v_name}` is defined multiple times"),
                ));
                continue;
            }
            if let Some(range) = anonymous {
                let konst = self.items.consts.alloc(Const {
                    name: None,
                    ty: TypeRef::Path(Path::single(Symbol::intern(repr_name))),
                    ty_range: range,
                    loc: ItemLoc {
                        module: ctx.module,
                        file: ctx.file,
                        vis: Visibility::Public,
                    },
                    owner: None,
                    node: v.syntax().id(),
                    has_body: true,
                    def_range: range,
                    item_range: range,
                    discriminant: Some((strukt, variants.len() as u32)),
                    is_static: None,
                    extern_symbol: None,
                    specialization: false,
                    anon: None,
                    macro_scope: self.macro_scope,
                });
                discriminant = Discriminant::Const { konst, offset: 0 };
            }
            next = match discriminant {
                Discriminant::Value(value) => value
                    .checked_add(1)
                    .filter(|n| values.contains(n))
                    .map(Discriminant::Value),
                // Overflow is found once the constant's value is known.
                Discriminant::Const { konst, offset } => Some(Discriminant::Const {
                    konst,
                    offset: offset + 1,
                }),
            };
            let index = variants.len() as u32;
            defaults.extend(marked.into_iter().map(|(a, v, n)| (index, a, v, n)));
            variants.push(Variant {
                non_exhaustive: variant_non_exhaustive,
                name: v_name,
                kind,
                fields: fields.into(),
                discriminant,
                name_range,
                def_range: v_range,
            });
        }
        if written && has_fields && repr.is_none() {
            self.items.diagnostics.push(Diagnostic::new(
                def_range,
                "`#[repr(inttype)]` must be specified for enums with explicit discriminants and non-unit variants",
            ));
        }
        let name_range = name.syntax().text_range();
        let id = self.items.structs.alloc(Struct {
            is_union: false,
            non_exhaustive,
            name: Symbol::intern(name.text()),
            generics,
            kind: StructKind::Record,
            fields: Box::default(),
            variants: Some(variants.into()),
            repr,
            repr_c: hints.iter().any(|(h, _)| h.as_str() == "C"),
            repr_transparent: false,
            repr_align,
            repr_pack,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis,
            },
            name_range,
            def_range,
        });
        if let Some(lang) = lang {
            self.items.lang_enums.insert(lang, id);
        }
        let derives_default = derives.iter().find(|(d, _)| *d == Derive::Default);
        let mut default_ok = true;
        match derives_default {
            // `#[default]` is the derive's helper attribute: without it,
            // an unknown attribute.
            None => {
                for &(_, _, _, name) in &defaults {
                    self.items.diagnostics.push(Diagnostic::new(
                        name,
                        "cannot find attribute `default` in this scope",
                    ));
                }
            }
            Some(&(_, at)) => {
                let variants = self.items.structs[id]
                    .variants
                    .as_deref()
                    .unwrap_or_default();
                for &(_, attr, has_value, _) in &defaults {
                    if has_value {
                        self.items.diagnostics.push(Diagnostic::new(
                            attr,
                            "`#[default]` attribute does not accept a value",
                        ));
                    }
                }
                match defaults.as_slice() {
                    [] => {
                        self.items.diagnostics.push(Diagnostic::new(
                            at,
                            "`#[derive(Default)]` on enum with no `#[default]`",
                        ));
                        default_ok = false;
                    }
                    &[(v, ..)] => {
                        let variant = &variants[v as usize];
                        if variant.non_exhaustive {
                            self.items.diagnostics.push(Diagnostic::new(
                                variant.name_range,
                                "default enum variant cannot be non-exhaustive",
                            ));
                            default_ok = false;
                        } else if variant.kind == StructKind::Unit {
                            self.items.default_variants.insert(id, v);
                        } else {
                            self.items.diagnostics.push(Diagnostic::new(
                                variant.name_range,
                                "the `#[default]` attribute may only be used on unit enum variants",
                            ));
                            default_ok = false;
                        }
                    }
                    _ => {
                        self.items
                            .diagnostics
                            .push(Diagnostic::new(at, "multiple declared defaults"));
                        default_ok = false;
                    }
                }
            }
        }
        for (derive, range) in derives {
            if derive == Derive::Default && !default_ok {
                continue;
            }
            self.derives.push((ctx, id, derive, range, e.syntax().id()));
        }
        self.adt_ids.insert((ctx.file, e.syntax().id()), id);
        self.ctors.push((ctx, id, e.syntax().id()));
    }

    /// A function for each tuple struct's and tuple variant's
    /// constructor, with the struct's generics, its fields as parameters
    /// and the struct as its type: what the constructor's path is as a
    /// value (`.map(Some)`), as rustc's constructors are functions.
    fn make_ctors(&mut self) {
        let ctors = std::mem::take(&mut self.ctors);
        for (ctx, strukt, node) in ctors {
            let s = &self.items.structs[strukt];
            let variants: Vec<(u32, Symbol, TextRange, Box<[Field]>)> = match &s.variants {
                None if s.kind == StructKind::Tuple => {
                    vec![(0, s.name, s.name_range, s.fields.clone())]
                }
                None => Vec::new(),
                Some(variants) => variants
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.kind == StructKind::Tuple)
                    .map(|(i, v)| (i as u32, v.name, v.name_range, v.fields.clone()))
                    .collect(),
            };
            if variants.is_empty() {
                continue;
            }
            let generics = Generics {
                params: s
                    .generics
                    .params
                    .iter()
                    .map(|p| GenericParam {
                        default: None,
                        ..p.clone()
                    })
                    .collect(),
                ..s.generics.clone()
            };
            let mut self_path = Path::single(s.name);
            self_path.ranges = Box::new([s.name_range]);
            if !generics.params.is_empty() || !generics.lifetimes.is_empty() {
                self_path.args = Box::new([Some(GenericArgs {
                    lifetimes: generics
                        .lifetimes
                        .iter()
                        .map(|l| LifetimeRef {
                            name: l.name,
                            range: l.range,
                        })
                        .collect(),
                    types: generics
                        .params
                        .iter()
                        .map(|p| TypeRef::Path(Path::single(p.name)))
                        .collect(),
                    ranges: generics.params.iter().map(|p| p.range).collect(),
                    bindings: Box::default(),
                    range: s.generics.range,
                    parenthesized: false,
                    bound_lifetimes: Box::default(),
                })]);
            }
            for (v, name, range, fields) in variants {
                let params: Box<[Param]> = fields
                    .iter()
                    .map(|f| Param {
                        ty: f.ty.clone(),
                        name: None,
                        range: f.range,
                        whole: f.def_range,
                    })
                    .collect();
                let function = self.items.functions.alloc(Function {
                    name,
                    generics: generics.clone(),
                    params,
                    ret: TypeRef::Path(self_path.clone()),
                    ret_range: range,
                    loc: ItemLoc {
                        module: ctx.module,
                        file: ctx.file,
                        vis: Visibility::Public,
                    },
                    is_unsafe: false,
                    target_features: None,
                    link_name: None,
                    is_extern: false,
                    abi: Abi::Rust,
                    is_const: true,
                    inherits_overflow_checks: false,
                    const_unstable: false,
                    precise_live_drops: false,
                    intrinsic: None,
                    legacy_const_generic: None,
                    is_variadic: false,
                    track_caller: false,
                    inline: None,
                    cold: false,
                    do_not_const_check: false,
                    owner: None,
                    has_self: false,
                    has_body: true,
                    node,
                    name_range: range,
                    def_range: range,
                    sig_range: range,
                    item_range: range,
                    param_list: range,
                    derived: None,
                    ctor: Some((strukt, v)),
                    opaques: Box::default(),
                    macro_scope: None,
                });
                self.items.ctor_fns.insert((strukt, v), function);
            }
        }
    }

    fn expand_derives(&mut self) {
        let derives = std::mem::take(&mut self.derives);
        let copy: rustc_hash::FxHashSet<StructId> = derives
            .iter()
            .filter(|d| d.2 == Derive::Copy)
            .map(|d| d.1)
            .collect();
        for &(ctx, strukt, derive, range, node) in &derives {
            let copies = derive == Derive::Clone
                && self.items.structs[strukt].generics.params.is_empty()
                && copy.contains(&strukt);
            self.derive(ctx, strukt, derive, copies, range, node);
        }
    }

    /// The `impl` block `#[derive(Tr)]` (written at `range`) makes for
    /// struct `strukt`, whose node is `node`: rustc's `impl<T: Clone>
    /// Clone for S<T>`, with a method whose body is made from the fields.
    fn derive(
        &mut self,
        ctx: Ctx,
        strukt: StructId,
        derive: Derive,
        copies: bool,
        range: TextRange,
        node: NodeId,
    ) {
        let s = &self.items.structs[strukt];
        let (module, name) = match derive {
            Derive::Clone => ("clone", "Clone"),
            Derive::Copy => ("marker", "Copy"),
            Derive::PartialEq => ("cmp", "PartialEq"),
            Derive::Eq => ("cmp", "Eq"),
            Derive::PartialOrd => ("cmp", "PartialOrd"),
            Derive::Ord => ("cmp", "Ord"),
            Derive::Default => ("default", "Default"),
            Derive::Debug => ("fmt", "Debug"),
            Derive::Hash => ("hash", "Hash"),
        };
        // rustc's `Default` for an enum bounds no parameters: its
        // default variant has no fields.
        let bounds_params = !(derive == Derive::Default && s.is_enum());
        let in_core = self.items.is_core(ctx.module);
        let trait_path = core_path(module, name, in_core);
        let bound = Bound {
            bound_lifetimes: Box::default(),
            path: trait_path,
            range,
        };
        let params: Box<[GenericParam]> = s
            .generics
            .params
            .iter()
            .map(|p| GenericParam {
                name: p.name,
                range: p.range,
                default: None,
                synthetic: false,
                kind: p.kind.clone(),
                maybe_unsized: p.maybe_unsized,
                may_dangle: false,
            })
            .collect();
        // As rustc expands it: each parameter's own bounds and the
        // trait's, then the struct's `where` clause.
        let inline = |p: &WherePredicate| p.ty_range.end() <= s.generics.range.end();
        let mut predicates = Vec::new();
        for param in params.iter().filter(|p| !p.is_const()) {
            let ty = TypeRef::Path(Path::single(param.name));
            predicates.extend(
                s.generics
                    .predicates
                    .iter()
                    .filter(|p| inline(p) && p.ty == ty)
                    .cloned(),
            );
            if bounds_params {
                predicates.push(WherePredicate {
                    ty,
                    ty_range: param.range,
                    bound: bound.clone(),
                });
            }
        }
        if s.repr_pack.is_some() && !matches!(derive, Derive::Default | Derive::Copy) {
            for param in params.iter().filter(|p| !p.is_const()) {
                predicates.push(WherePredicate {
                    ty: TypeRef::Path(Path::single(param.name)),
                    ty_range: param.range,
                    bound: Bound {
                        path: core_path("marker", "Copy", in_core),
                        range,
                        bound_lifetimes: Box::default(),
                    },
                });
            }
        }
        predicates.extend(s.generics.predicates.iter().filter(|p| !inline(p)).cloned());
        // And the trait for each field type's associated types of the
        // parameters (`T::Out`), which a bound on `T` cannot give.
        let mut projections = Vec::new();
        let variant_fields = s.variants.iter().flatten().flat_map(|v| v.fields.iter());
        for field in s.fields.iter().chain(variant_fields) {
            if bounds_params {
                param_projections(&field.ty, &params, &mut projections);
            }
        }
        for ty in projections {
            predicates.push(WherePredicate {
                ty,
                ty_range: range,
                bound: bound.clone(),
            });
        }
        let mut self_path = Path::single(s.name);
        self_path.ranges = Box::new([s.name_range]);
        if !params.is_empty() || !s.generics.lifetimes.is_empty() {
            self_path.args = Box::new([Some(GenericArgs {
                lifetimes: s
                    .generics
                    .lifetimes
                    .iter()
                    .map(|l| LifetimeRef {
                        name: l.name,
                        range: l.range,
                    })
                    .collect(),
                types: params
                    .iter()
                    .map(|p| TypeRef::Path(Path::single(p.name)))
                    .collect(),
                ranges: params.iter().map(|p| p.range).collect(),
                bindings: Box::default(),
                range: s.generics.range,
                parenthesized: false,
                bound_lifetimes: Box::default(),
            })]);
        }
        let generics = Generics {
            params,
            lifetimes: s.generics.lifetimes.clone(),
            predicates: predicates.into(),
            outlives: s.generics.outlives.clone(),
            type_outlives: s.generics.type_outlives.clone(),
            range: s.generics.range,
        };
        let name_range = s.name_range;
        let id = self.items.impls.alloc(Impl {
            generics,
            trait_ref: Some(bound),
            trait_: None,
            is_unsafe: false,
            self_ty: TypeRef::Path(self_path),
            self_range: name_range,
            def_range: range,
            module: ctx.module,
            file: ctx.file,
            target: None,
            functions: Vec::new(),
            consts: Vec::new(),
            types: Vec::new(),
        });
        let self_ty = || TypeRef::Path(Path::single(kw::SelfUpper));
        let self_ref = || Param {
            ty: TypeRef::Ref {
                lifetime: None,
                amp: range,
                mutable: false,
                pointee: Box::new(self_ty()),
            },
            name: None,
            range,
            whole: range,
        };
        // `hash<__H: Hasher>`'s parameter.
        let mut method_generics = Generics::default();
        let (method, params, ret) = match derive {
            Derive::Copy => return,
            // `hash<__H: Hasher>(&self, state: &mut __H)`
            Derive::Hash => {
                let h = Symbol::intern("__H");
                method_generics = Generics {
                    params: Box::new([GenericParam {
                        name: h,
                        range,
                        default: None,
                        synthetic: false,
                        kind: ParamKind::Type,
                        maybe_unsized: false,
                        may_dangle: false,
                    }]),
                    predicates: Box::new([WherePredicate {
                        ty: TypeRef::Path(Path::single(h)),
                        ty_range: range,
                        bound: Bound {
                            bound_lifetimes: Box::default(),
                            path: core_path("hash", "Hasher", in_core),
                            range,
                        },
                    }]),
                    ..Generics::default()
                };
                let state = Param {
                    ty: TypeRef::Ref {
                        lifetime: None,
                        amp: range,
                        mutable: true,
                        pointee: Box::new(TypeRef::Path(Path::single(h))),
                    },
                    name: Some(Symbol::intern("state")),
                    range,
                    whole: range,
                };
                ("hash", vec![self_ref(), state], TypeRef::unit())
            }
            Derive::Clone => ("clone", vec![self_ref()], self_ty()),
            Derive::PartialEq => (
                "eq",
                vec![self_ref(), self_ref()],
                TypeRef::Path(Path::single(Symbol::intern("bool"))),
            ),
            Derive::Eq => (
                "assert_receiver_is_total_eq",
                vec![self_ref()],
                TypeRef::unit(),
            ),
            Derive::PartialOrd => {
                // `Option<Ordering>`
                let mut option = core_path("option", "Option", in_core);
                // On the last segment: `::core::option::Option`, or
                // `crate::option::Option` in `core`.
                let leading = std::iter::repeat_n(None, option.segments.len() - 1);
                option.args = leading
                    .chain([Some(GenericArgs {
                        lifetimes: Box::default(),
                        types: Box::new([TypeRef::Path(core_path("cmp", "Ordering", in_core))]),
                        ranges: Box::new([range]),
                        bindings: Box::default(),
                        range,
                        parenthesized: false,
                        bound_lifetimes: Box::default(),
                    })])
                    .collect();
                (
                    "partial_cmp",
                    vec![self_ref(), self_ref()],
                    TypeRef::Path(option),
                )
            }
            Derive::Ord => (
                "cmp",
                vec![self_ref(), self_ref()],
                TypeRef::Path(core_path("cmp", "Ordering", in_core)),
            ),
            Derive::Default => ("default", Vec::new(), self_ty()),
            // `fmt(&self, f: &mut Formatter<'_>) -> fmt::Result`
            Derive::Debug => {
                let formatter = Param {
                    ty: TypeRef::Ref {
                        lifetime: None,
                        amp: range,
                        mutable: true,
                        pointee: Box::new(TypeRef::Path(core_path("fmt", "Formatter", in_core))),
                    },
                    name: Some(Symbol::intern("f")),
                    range,
                    whole: range,
                };
                (
                    "fmt",
                    vec![self_ref(), formatter],
                    TypeRef::Path(core_path("fmt", "Result", in_core)),
                )
            }
        };
        let has_self = !params.is_empty();
        let function = self.items.functions.alloc(Function {
            name: Symbol::intern(method),
            generics: method_generics,
            params: params.into(),
            ret,
            ret_range: range,
            loc: ItemLoc {
                module: ctx.module,
                file: ctx.file,
                vis: Visibility::Public,
            },
            is_unsafe: false,
            target_features: None,
            link_name: None,
            is_extern: false,
            abi: Abi::Rust,
            is_const: false,
            inherits_overflow_checks: false,
            const_unstable: false,
            precise_live_drops: false,
            intrinsic: None,
            legacy_const_generic: None,
            track_caller: false,
            inline: None,
            cold: false,
            do_not_const_check: false,
            is_variadic: false,
            owner: Some(Owner::Impl(id)),
            has_self,
            has_body: true,
            node,
            name_range: range,
            def_range: range,
            sig_range: range,
            item_range: range,
            param_list: range,
            derived: Some((strukt, if copies { Derive::Copy } else { derive })),
            ctor: None,
            opaques: Box::default(),
            macro_scope: None,
        });
        self.items.impls[id].functions.push(function);
    }

    fn collect_extern_block(&mut self, ctx: Ctx, block: ast::ExternBlock<'_>) {
        let edition = self.db.file(ctx.file).edition;
        let diagnostics = &mut self.items.diagnostics;
        let attrs = crate::cfg::Attrs::new(block.attrs());
        diagnostics.extend(
            attrs
                .errors
                .iter()
                .map(|(range, message)| Diagnostic::new(*range, message.clone())),
        );
        for attr in attrs.iter() {
            if attr_name(attr) == Some("link") {
                if let Some(name) = super::attributes::link_attr(attr, diagnostics)
                    && !self.items.native_libraries.contains(&name)
                {
                    self.items.native_libraries.push(name);
                }
            } else {
                super::attributes::check_attr(attr, diagnostics);
            }
        }
        let abi = block.abi();
        let mut block_abi = function_abi(abi, diagnostics);
        if block_abi == Abi::Rust {
            // Reported above unless the block really says "Rust".
            if let Some(abi) = abi
                && abi.string_token().is_some_and(|s| s.text() == "\"Rust\"")
            {
                unsupported(
                    diagnostics,
                    abi.syntax().text_range(),
                    "`extern \"Rust\"` blocks",
                );
            }
            block_abi = Abi::C;
        }
        if edition == flux_syntax::Edition::E2024 && block.unsafe_token().is_none() {
            let range = abi.map_or(block.syntax().text_range(), |a| a.syntax().text_range());
            diagnostics.push(Diagnostic::new(
                range,
                "`extern` blocks must be `unsafe` in edition 2024",
            ));
        }
        let Some(list) = block.extern_item_list() else {
            return;
        };
        for item in list.items() {
            if !item_enabled(item.syntax(), &mut self.items.diagnostics) {
                continue;
            }
            match item {
                ast::Item::Fn(f) => {
                    self.collect_fn(ctx, f, Some(block_abi), None);
                }
                ast::Item::Static(st) => self.collect_static(ctx, st, true),
                _ => self.items.diagnostics.push(Diagnostic::new(
                    item.syntax().text_range(),
                    "only functions and statics can be declared in `extern` blocks",
                )),
            }
        }
    }
}
