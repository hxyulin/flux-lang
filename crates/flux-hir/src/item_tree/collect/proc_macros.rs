//! Procedural attribute replacement and custom-derive item collection.
use super::{
    AstNode, Collector, Ctx, Diagnostic, MacroId, NodeId, SourceDb, Symbol, SyntaxKind, TextRange,
    ast, attr_name, is_item_kind, item_enabled, macros,
};
use flux_syntax::T;

impl Collector<'_> {
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

    /// Attribute macros replace items; derives append items after retaining the
    /// configured input. Helpers are consumed only for a resolved derive.
    pub(super) fn collect_proc_item(&mut self, ctx: Ctx, node: NodeId) -> bool {
        if !self.db.has_proc_macros() {
            return false;
        }
        let shared = self.db.shared(ctx.file);
        let syntax = shared.parse.tree().node(node);
        let attrs = crate::cfg::Attrs::new(syntax.children().filter_map(ast::Attr::cast));
        let mut active = attrs.iter().collect::<Vec<_>>();
        let mut attribute = None;
        for (index, attr) in active.iter().enumerate() {
            let Some(path) = attr.meta().and_then(|m| m.path()) else {
                continue;
            };
            let mut toks = Vec::new();
            macros::tt::tokens_of(path.syntax(), &mut toks);
            if let Some(id) = self.proc_path(ctx, &toks)
                && let macros::MacroKind::ProcMacro(host) = &self.items.macros[id].kind
                && host.kind == crate::ProcMacroKind::Attribute
            {
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
        let mut helper_names = rustc_hash::FxHashSet::default();
        if attribute.is_none() {
            for attr in &active {
                if attr_name(*attr) != Some("derive") {
                    macros::tt::tokens_of(attr.syntax(), &mut replacement);
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
                            helper_names.extend(host.helpers.iter().copied());
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
        if attribute.is_none() {
            for child in syntax
                .descendants()
                .skip(1)
                .filter(|n| crate::item_tree::cfg_position(*n))
            {
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
                }
            }
        }
        let mut derive_input = replacement.clone();
        macros::tt::proc_tokens_ignoring(syntax, &skip, &mut derive_input);
        let mut body = Vec::new();
        if let Some((index, _)) = attribute {
            active.remove(index);
            for attr in &active {
                macros::tt::tokens_of(attr.syntax(), &mut replacement);
            }
        } else {
            // Only declared helper attributes on the deriving item, its fields
            // and variants are inert. Unknown attributes still reach admission.
            for attr in syntax.descendants().filter_map(ast::Attr::cast) {
                if attr_name(attr).is_some_and(|n| helper_names.contains(&Symbol::intern(n))) {
                    let parent = attr.syntax().parent();
                    if parent.is_some_and(|p| {
                        matches!(
                            p.kind(),
                            SyntaxKind::STRUCT
                                | SyntaxKind::ENUM
                                | SyntaxKind::UNION
                                | SyntaxKind::VARIANT
                                | SyntaxKind::RECORD_FIELD
                                | SyntaxKind::TUPLE_FIELD
                        )
                    }) {
                        skip.push(attr.syntax().raw_range());
                    }
                }
            }
            // root helper attributes were copied to replacement; remove those
            // by copying only the active non-helper attributes once more below.
            if !helper_names.is_empty() {
                // Remove helper groups from the already assembled root prefix.
                let names = &helper_names;
                let mut filtered = Vec::new();
                let mut pos = 0;
                while pos < replacement.len() {
                    let start = pos;
                    if replacement[pos].kind == T![#]
                        && replacement.get(pos + 1).is_some_and(|t| t.kind == T!['['])
                    {
                        pos += 2;
                        let mut depth = 1;
                        while pos < replacement.len() && depth != 0 {
                            match replacement[pos].kind {
                                T!['['] => depth += 1,
                                T![']'] => depth -= 1,
                                _ => {}
                            }
                            pos += 1;
                        }
                        if replacement
                            .get(start + 2)
                            .is_some_and(|t| names.contains(&t.text))
                        {
                            continue;
                        }
                    } else {
                        pos += 1;
                    }
                    filtered.extend_from_slice(&replacement[start..pos]);
                }
                replacement = filtered;
            }
        }
        macros::tt::proc_tokens_ignoring(syntax, &skip, &mut body);
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
            self.items.files.insert(file, self.db.shared(file));
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
