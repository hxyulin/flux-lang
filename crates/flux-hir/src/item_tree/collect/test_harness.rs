//! Test discovery runs after macro expansion, before name resolution.
use super::*;
use std::collections::BTreeMap;

impl Collector<'_> {
    pub(super) fn make_test_harness(&mut self, root: FileId) {
        if !self
            .db
            .file(root)
            .parse
            .tree()
            .cfg_options()
            .is_some_and(|cfg| {
                cfg.names
                    .iter()
                    .any(|name| &**name == "__flux_test_harness")
            })
        {
            return;
        }
        let krate = CrateId(self.items.crates.len() as u32 - 1);
        let root_module = self.items.module_of(krate);
        let mut generated = BTreeMap::<ModuleId, String>::new();
        let mut registrations = BTreeMap::<ModuleId, String>::new();
        let mut functions: Vec<_> = self
            .fn_ids
            .iter()
            .map(|(&(file, node), &id)| (id, file, node))
            .collect();
        functions.sort_by_key(|entry| entry.0);
        for (id, file, node) in functions {
            let function = &self.items.functions[id];
            if self.items.modules[function.loc.module].krate != krate
                || self.items.modules[function.loc.module].block.is_some()
                || function.owner.is_some()
            {
                continue;
            }
            let shared = self.db.shared(file);
            let Some(f) = ast::Fn::cast(shared.parse.tree().node(node)) else {
                continue;
            };
            let attrs = crate::cfg::Attrs::new(f.attrs());
            if !attrs.iter().any(|attr| attr_name(attr) == Some("test")) {
                continue;
            }
            if function.is_unsafe
                || function.is_async
                || !function.params.is_empty()
                || !function.generics.params.is_empty()
            {
                self.items.diagnostics.push(Diagnostic { file, range: f.syntax().text_range(), message: "test functions must be safe, synchronous and take no parameters or generics".into() });
                continue;
            }
            let mut expected = "None".to_owned();
            let mut ignored = false;
            for attr in attrs.iter() {
                match attr_name(attr) {
                    Some("ignore") => ignored = true,
                    Some("should_panic") => {
                        expected = "Some(None)".into();
                        if let Some(tree) = attr.meta().and_then(|meta| meta.token_tree()) {
                            let mut tokens = Vec::new();
                            macros::tt::tokens_of(tree.syntax(), &mut tokens);
                            if let Some(token) =
                                tokens.iter().find(|token| token.kind == SyntaxKind::STRING)
                                && let Ok(message) =
                                    crate::literal::unescape_str(token.text.as_str())
                            {
                                expected = format!("Some(Some({message:?}))");
                            }
                        }
                    }
                    _ => {}
                }
            }
            let module = function.loc.module;
            let wrapper = format!("__flux_test_case_{}", id.index());
            generated.entry(module).or_default().push_str(&format!(
                "fn {wrapper}() {{ crate::__flux_test_runtime::check_result(r#{}()); }}\n",
                function.name
            ));
            let mut parts = vec![function.name.to_string()];
            let mut at = module;
            while let Some(parent) = self.items.modules[at].parent {
                parts.push(self.items.modules[at].name.to_string());
                at = parent;
            }
            parts.reverse();
            let name = parts.join("::");
            registrations.entry(module).or_default().push_str(&format!("tests.push(crate::__flux_test_runtime::Test {{name: {name:?}, run: {wrapper}, ignored: {ignored}, expected: {expected}}});\n"));
        }
        let modules: Vec<_> = self
            .items
            .modules
            .iter()
            .filter(|(_, module)| module.krate == krate && module.block.is_none())
            .map(|(id, module)| (id, module.parent, module.name))
            .collect();
        for &(module, parent, name) in &modules {
            generated.entry(module).or_default();
            if let Some(parent) = parent {
                registrations
                    .entry(parent)
                    .or_default()
                    .push_str(&format!("self::r#{name}::__flux_collect_tests(tests);\n"));
            }
        }
        for (module, mut text) in generated {
            text.push_str("pub(crate) fn __flux_collect_tests(tests: &mut ::std::vec::Vec<crate::__flux_test_runtime::Test>) {\n");
            text.push_str(registrations.get(&module).map_or("", String::as_str));
            text.push_str("}\n");
            if module == root_module {
                if self.items.crates[krate.0 as usize].no_std
                    && !self.items.std_available(root_module)
                {
                    text.push_str("extern crate std;\n");
                }
                text.push_str(include_str!("../../../../../library/test/harness.rs.txt"));
            }
            let parse = flux_syntax::parse_expansion_with_edition(
                text.into_boxed_str(),
                &[],
                TextRange::empty(0),
                flux_syntax::parser::Fragment::Items,
                self.db.file(root).edition,
            );
            let file = self.db.add_expansion(parse, root);
            self.items.expansion_files.insert(file, root);
            let shared = self.db.shared(file);
            self.items.insert_file(file, shared.clone());
            let nodes = shared
                .parse
                .root()
                .children()
                .filter(|node| is_item_kind(node.kind()))
                .map(|node| node.id())
                .collect::<Vec<_>>();
            let start = self.items.diagnostics.len();
            self.collect_items(Ctx { module, file }, &nodes);
            self.tag(start, file);
        }
    }
}
