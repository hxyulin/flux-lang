//! Procedural attribute replacement and custom-derive item collection.
use super::{
    AssocOwner, AstNode, Collector, Ctx, Derive, Diagnostic, MacroId, NodeId, SourceDb, Symbol,
    SyntaxKind, SyntaxNode, TextRange, ast, attr_name, check_attr, is_item_kind, item_enabled,
    macros,
};
use flux_syntax::T;
use rustc_hash::FxHashSet;

fn is_derive_helper(attr: ast::Attr<'_>, names: &FxHashSet<Symbol>) -> bool {
    attr.meta()
        .is_some_and(|meta| meta.unsafe_token().is_none())
        && attr_name(attr).is_some_and(|name| names.contains(&Symbol::intern(name)))
}

impl Collector<'_> {
    /// Imports through local modules need the completed macro namespace.
    /// Keep the whole item so a late derive can still claim its helpers.
    pub(super) fn proc_item_needs_scope(&self, ctx: Ctx, syntax: SyntaxNode<'_>) -> bool {
        if !self.db.has_proc_macros() {
            return false;
        }
        let attrs = crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast));
        attrs.iter().any(|attr| {
            let Some(meta) = attr.meta() else {
                return false;
            };
            if attr_name(attr) == Some("derive") {
                let Some(tree) = meta.token_tree() else {
                    return false;
                };
                let mut tokens = Vec::new();
                macros::tt::tokens_of(tree.syntax(), &mut tokens);
                return macros::strip_delimiters(&tokens)
                    .split(|token| token.kind == T![,])
                    .filter(|path| !path.is_empty())
                    .any(|path| {
                        !matches!(path, [name] if Derive::named(name.text.as_str()).is_some())
                            && self.proc_path(ctx, path).is_none()
                    });
            }
            let Some(path) = meta.path().filter(|path| path.qualifier().is_some()) else {
                return false;
            };
            if self.is_derive_attr(ctx, attr) {
                return false;
            }
            let mut errors = Vec::new();
            check_attr(attr, &mut errors);
            if errors.is_empty() {
                return false;
            }
            let mut tokens = Vec::new();
            macros::tt::tokens_of(path.syntax(), &mut tokens);
            self.proc_path(ctx, &tokens).is_none()
        })
    }

    pub(super) fn proc_path(&self, ctx: Ctx, tokens: &[macros::tt::Tok]) -> Option<MacroId> {
        let mut starts = Vec::new();
        let text = macros::tt::write_text(tokens, &mut starts);
        let parse = flux_syntax::parse_included_source(
            text.into(),
            flux_syntax::parser::Fragment::Type,
            self.items.edition(ctx.module),
        );
        if !parse.errors().is_empty() {
            return None;
        }
        let path = parse
            .root()
            .children()
            .next()?
            .children()
            .find_map(ast::Path::cast)?;
        let macros::Resolved::Def(id) =
            macros::resolve::<()>(self.items, path, &[], self.macro_scope, ctx.module).ok()?
        else {
            return None;
        };
        matches!(self.items.macros[id].kind, macros::MacroKind::ProcMacro(_)).then_some(id)
    }

    fn proc_attribute(&self, ctx: Ctx, attr: ast::Attr<'_>) -> Option<MacroId> {
        let path = attr.meta()?.path()?;
        let mut tokens = Vec::new();
        macros::tt::tokens_of(path.syntax(), &mut tokens);
        let id = self.proc_path(ctx, &tokens)?;
        matches!(&self.items.macros[id].kind, macros::MacroKind::ProcMacro(host)
            if host.kind == crate::ProcMacroKind::Attribute)
        .then_some(id)
    }

    /// The original types of an attributed associated item are replaced along
    /// with its body. Type macros inside it are part of the attribute input.
    pub(super) fn assoc_proc_replaces_type(
        &self,
        ctx: Ctx,
        mut node: SyntaxNode<'_>,
        assoc_root: bool,
    ) -> bool {
        if !self.db.has_proc_macros() {
            return false;
        }
        while let Some(parent) = node.parent() {
            if is_item_kind(node.kind())
                && (parent.kind() == SyntaxKind::ASSOC_ITEM_LIST
                    || assoc_root && parent.kind() == SyntaxKind::SOURCE_FILE)
            {
                let attrs = crate::cfg::Attrs::new(node.children().filter_map(ast::Attr::cast));
                if attrs
                    .iter()
                    .any(|attr| self.proc_attribute(ctx, attr).is_some())
                    || self.proc_item_needs_scope(ctx, node)
                {
                    return true;
                }
            }
            node = parent;
        }
        false
    }

    /// Attribute macros replace items; derives append items after retaining the
    /// configured input. Helpers are consumed only for a resolved derive.
    pub(super) fn collect_proc_item(
        &mut self,
        ctx: Ctx,
        node: NodeId,
        owner: Option<AssocOwner>,
    ) -> bool {
        if !self.db.has_proc_macros() {
            return false;
        }
        let shared = self.db.shared(ctx.file);
        let syntax = shared.parse.tree().node(node);
        let attrs = crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast));
        let mut active = attrs.iter().collect::<Vec<_>>();
        let mut helper_names = FxHashSet::default();
        let mut unavailable_helpers = false;
        for attr in active
            .iter()
            .filter(|attr| attr_name(**attr) == Some("derive"))
        {
            let Some(tree) = attr.meta().and_then(|meta| meta.token_tree()) else {
                continue;
            };
            let mut tokens = Vec::new();
            macros::tt::tokens_of(tree.syntax(), &mut tokens);
            for path in macros::strip_delimiters(&tokens).split(|t| t.kind == T![,]) {
                if let Some(id) = self.proc_path(ctx, path)
                    && let macros::MacroKind::ProcMacro(host) = &self.items.macros[id].kind
                    && host.kind == crate::ProcMacroKind::Derive
                {
                    if let Some(helpers) = &host.helpers {
                        helper_names.extend(helpers.iter().copied());
                    } else {
                        unavailable_helpers = true;
                    }
                }
            }
        }
        if unavailable_helpers {
            // Helper names can shadow both builtin attributes and imported
            // attribute macros. Do not execute or interpret an ambiguous bare
            // attribute before knowing whether the deriving macro declares it.
            let mut enabled_from = 0;
            for node in syntax.descendants().filter(|node| {
                node.id() == syntax.id()
                    || matches!(
                        node.kind(),
                        SyntaxKind::STRUCT
                            | SyntaxKind::ENUM
                            | SyntaxKind::UNION
                            | SyntaxKind::VARIANT
                            | SyntaxKind::RECORD_FIELD
                            | SyntaxKind::TUPLE_FIELD
                    )
            }) {
                if node.raw_range().start() < enabled_from {
                    continue;
                }
                if !item_enabled(node, &mut self.items.diagnostics) {
                    enabled_from = node.raw_range().end();
                    continue;
                }
                let attrs = crate::cfg::Attrs::new(node.children().filter_map(ast::Attr::cast));
                for attr in attrs.iter() {
                    if let Some(name) = attr_name(attr)
                        && !matches!(name, "derive" | "cfg" | "cfg_attr")
                    {
                        self.items.diagnostics.push(Diagnostic::new(
                            attr.syntax().text_range(),
                            "bare attributes on derives without helper metadata are not supported yet",
                        ));
                        return true;
                    }
                }
            }
        }
        let mut attribute = None;
        for (index, attr) in active.iter().enumerate() {
            if attr_name(*attr) == Some("derive")
                && !matches!(
                    syntax.kind(),
                    SyntaxKind::STRUCT | SyntaxKind::ENUM | SyntaxKind::UNION
                )
            {
                self.items.diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    "derive may only be applied to structs, enums and unions",
                ));
                return true;
            }
            // Derive helpers occupy the item's attribute scope regardless of
            // where the derive appears in the list.
            if is_derive_helper(*attr, &helper_names) {
                continue;
            }
            if let Some(id) = self.proc_attribute(ctx, *attr) {
                if attr
                    .meta()
                    .is_some_and(|m| m.eq_token().is_some() || m.unsafe_token().is_some())
                {
                    self.items.diagnostics.push(Diagnostic::new(
                        attr.syntax().text_range(),
                        "expected a bare procedural attribute or a parenthesized list",
                    ));
                    return true;
                }
                attribute = Some((index, id));
                break;
            }
        }
        let mut derives = Vec::new();
        let mut replacement = Vec::new();
        let mut root_helpers = Vec::new();
        if attribute.is_none() {
            for attr in &active {
                if attr_name(*attr) != Some("derive") {
                    let start = replacement.len();
                    macros::tt::tokens_of(attr.syntax(), &mut replacement);
                    if is_derive_helper(*attr, &helper_names) {
                        root_helpers.push(start..replacement.len());
                    }
                    continue;
                }
                let Some(tree) = attr.meta().and_then(|m| m.token_tree()) else {
                    macros::tt::tokens_of(attr.syntax(), &mut replacement);
                    continue;
                };
                let mut toks = Vec::new();
                macros::tt::tokens_of(tree.syntax(), &mut toks);
                let toks = macros::strip_delimiters(&toks);
                let mut kept = Vec::new();
                let mut from = 0;
                let mut depth = 0;
                for pos in 0..=toks.len() {
                    if pos < toks.len() {
                        match toks[pos].kind {
                            T!['('] | T!['['] | T!['{'] => depth += 1,
                            T![')'] | T![']'] | T!['}'] => depth -= 1,
                            _ => {}
                        }
                    }
                    if pos == toks.len() || (toks[pos].kind == T![,] && depth == 0) {
                        let part = &toks[from..pos];
                        from = pos + 1;
                        if part.is_empty() {
                            if pos < toks.len() {
                                self.items.diagnostics.push(Diagnostic::new(
                                    attr.syntax().text_range(),
                                    "expected an identifier in derive",
                                ));
                                return true;
                            }
                            continue;
                        }
                        if let Some(id) = self.proc_path(ctx, part) {
                            let macros::MacroKind::ProcMacro(host) = &self.items.macros[id].kind
                            else {
                                unreachable!()
                            };
                            if host.kind != crate::ProcMacroKind::Derive {
                                self.items.diagnostics.push(Diagnostic::new(
                                    attr.syntax().text_range(),
                                    "expected a derive macro",
                                ));
                                return true;
                            }
                            derives.push((host.clone(), attr.syntax().text_range()));
                        } else {
                            if !kept.is_empty() {
                                let mut comma = part[0];
                                comma.kind = T![,];
                                comma.text = Symbol::intern(",");
                                comma.joint = false;
                                kept.push(comma);
                            }
                            kept.extend_from_slice(part);
                        }
                    }
                }
                if !kept.is_empty() {
                    let mut outer = Vec::new();
                    macros::tt::tokens_of(attr.syntax(), &mut outer);
                    let Some(open) = outer.iter().position(|t| t.kind == T!['(']) else {
                        continue;
                    };
                    replacement.extend_from_slice(&outer[..=open]);
                    replacement.extend(kept);
                    let close = outer.iter().rposition(|t| t.kind == T![')']).unwrap();
                    replacement.extend_from_slice(&outer[close..]);
                }
            }
            if derives.is_empty() {
                return false;
            }
            if !matches!(
                syntax.kind(),
                SyntaxKind::STRUCT | SyntaxKind::ENUM | SyntaxKind::UNION
            ) {
                self.items.diagnostics.push(Diagnostic::new(
                    syntax.text_range(),
                    "derive may only be applied to structs, enums and unions",
                ));
                return true;
            }
        }
        if self.expansion_depth as usize >= self.items.macro_recursion_limit(ctx.module) {
            self.items.diagnostics.push(Diagnostic::new(
                syntax.text_range(),
                "recursion limit reached during procedural macro expansion",
            ));
            return true;
        }
        let mut skip = syntax
            .children()
            .filter_map(ast::Attr::cast)
            .map(|a| a.syntax().raw_range())
            .collect::<Vec<_>>();
        let mut derive_attrs = Vec::new();
        let mut retained_attrs = Vec::new();
        if attribute.is_none() {
            let mut disabled_end = 0;
            for child in syntax
                .descendants()
                .skip(1)
                .filter(|n| crate::item_tree::cfg_position(*n))
            {
                if child.raw_range().start() < disabled_end {
                    continue;
                }
                if !item_enabled(child, &mut self.items.diagnostics) {
                    let mut range = child.raw_range();
                    // Separators belong to the surrounding list in some nodes.
                    if let Some(next) = syntax
                        .tokens()
                        .find(|t| !t.kind().is_trivia() && t.raw_range().start() >= range.end())
                        && next.kind() == T![,]
                    {
                        range = TextRange::new(range.start(), next.raw_range().end());
                    }
                    skip.push(range);
                    disabled_end = range.end();
                    continue;
                }
                let accepts_helpers = matches!(
                    child.kind(),
                    SyntaxKind::VARIANT | SyntaxKind::RECORD_FIELD | SyntaxKind::TUPLE_FIELD
                );
                for attr in child.children().filter_map(ast::Attr::cast) {
                    let range = attr.syntax().raw_range();
                    if attr_name(attr) == Some("cfg_attr") {
                        let configured = crate::cfg::Attrs::new([attr]);
                        let mut derive_tokens = Vec::new();
                        let mut retained_tokens = Vec::new();
                        for attr in configured.iter() {
                            let start = derive_tokens.len();
                            macros::tt::tokens_of(attr.syntax(), &mut derive_tokens);
                            if !accepts_helpers || !is_derive_helper(attr, &helper_names) {
                                retained_tokens.extend_from_slice(&derive_tokens[start..]);
                            }
                        }
                        derive_attrs.push((range, derive_tokens));
                        retained_attrs.push((range, retained_tokens));
                    } else if accepts_helpers && is_derive_helper(attr, &helper_names) {
                        retained_attrs.push((range, Vec::new()));
                    }
                }
            }
            // The token emitter consumes replacements in raw source order.
            derive_attrs.sort_unstable_by_key(|(range, _)| range.start());
            retained_attrs.sort_unstable_by_key(|(range, _)| range.start());
        }
        let mut derive_input = replacement.clone();
        macros::tt::configured_proc_tokens(syntax, &skip, &derive_attrs, &mut derive_input);
        let mut body = Vec::new();
        if let Some((index, _)) = attribute {
            active.remove(index);
            for attr in &active {
                macros::tt::tokens_of(attr.syntax(), &mut replacement);
            }
        } else {
            // Keep helpers on the derive's input and remove only those exact
            // bare attributes from the retained item.
            for range in root_helpers.into_iter().rev() {
                replacement.drain(range);
            }
        }
        macros::tt::configured_proc_tokens(syntax, &skip, &retained_attrs, &mut body);
        let mut input = replacement.clone();
        input.extend(body);
        let real = self.real_file(ctx.file);
        let site = macros::CallSite {
            read_text: SourceDb::read_include_str,
            scope: self.macro_scope,
            items: self.items,
            krate: self.items.modules[ctx.module].krate,
            module: ctx.module,
            real_file: real,
        };
        let mut diagnostics = Vec::new();
        let mut contexts = Vec::new();
        let mut output = if attribute.is_some() {
            Vec::new()
        } else {
            input.clone()
        };
        if let Some((index, id)) = attribute {
            let original = attrs.iter().nth(index).unwrap();
            let mut arguments = Vec::new();
            if let Some(tree) = original.meta().and_then(|m| m.token_tree()) {
                macros::tt::tokens_of(tree.syntax(), &mut arguments);
                arguments = macros::strip_delimiters(&arguments).to_vec();
            }
            let macros::MacroKind::ProcMacro(host) = &site.items.macros[id].kind else {
                unreachable!()
            };
            if let Some(tokens) = macros::expand_proc(
                host,
                &input,
                Some(&arguments),
                &site,
                syntax.text_range(),
                &mut contexts,
                &mut diagnostics,
            ) {
                output.extend(tokens);
            }
        } else {
            for (host, range) in derives {
                if let Some(tokens) = macros::expand_proc(
                    &host,
                    &derive_input,
                    None,
                    &site,
                    range,
                    &mut contexts,
                    &mut diagnostics,
                ) {
                    output.extend(tokens);
                }
            }
        }
        let parse =
            macros::parse_proc_output(&output, &site, syntax.text_range(), &mut diagnostics);
        self.items.diagnostics.extend(diagnostics);
        if let Some(parse) = parse {
            let file = self.db.add_expansion(parse, real);
            self.items.expansion_files.insert(file, real);
            if let Some(owner) = owner {
                self.collect_assoc_expansion(ctx, owner, file);
                return true;
            }
            self.items.insert_file(file, self.db.shared(file));
            let shared = self.db.shared(file);
            let nodes = shared
                .parse
                .root()
                .children()
                .filter(|n| is_item_kind(n.kind()))
                .map(|n| n.id())
                .collect::<Vec<_>>();
            self.expansion_depth += 1;
            self.collect_items(
                Ctx {
                    module: ctx.module,
                    file,
                },
                &nodes,
            );
            self.expansion_depth -= 1;
        }
        true
    }
}
