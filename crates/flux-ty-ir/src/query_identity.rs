//! Edit-stable definition paths for query caches, separate from runtime type identity.

use std::collections::{BTreeMap, BTreeSet};

use flux_hir::flib::IdKind;
use flux_hir::{CrateId, FileId, ItemTree, ModuleId, Owner};
use flux_syntax::{SyntaxKind, TextRange};

use crate::{Ty, TyKind, type_identity};

type Definition = (IdKind, u32);

// Bound recursive ownership traversal when an input graph has a cycle.
const MAX_IDENTITY_DEPTH: usize = 128;

/// Definition paths and their unambiguous reverse mappings in one source graph.
///
/// Paths identify definitions, not their contents or dependencies. Callers must
/// separately fingerprint the inputs of a cached query. Unlike runtime type and
/// artifact keys, these paths exclude crate source revisions and arena ordinals.
#[derive(Debug)]
pub struct DefinitionKeys {
    keys: BTreeMap<Definition, Vec<u8>>,
    definitions: BTreeMap<Vec<u8>, Definition>,
}

impl DefinitionKeys {
    /// Builds paths under explicit package, target and configuration namespaces.
    ///
    /// Each supplied namespace must be nonempty, unique to its crate and stable
    /// across source edits. A content hash of the crate is unsuitable. Definitions
    /// in crates without a namespace have no key. Duplicate paths, generated
    /// definitions, macro expansions and unsupported anonymous owners decline reuse.
    pub fn new(items: &ItemTree, namespaces: &[(CrateId, &[u8])]) -> Self {
        let mut builder = Builder {
            items,
            namespaces,
            keys: BTreeMap::new(),
            headers: BTreeMap::new(),
        };
        let definitions = items
            .modules
            .iter()
            .map(|(id, _)| (IdKind::Module, id.into_raw()))
            .chain(
                items
                    .functions
                    .iter()
                    .map(|(id, _)| (IdKind::Function, id.into_raw())),
            )
            .chain(
                items
                    .structs
                    .iter()
                    .map(|(id, _)| (IdKind::Struct, id.into_raw())),
            )
            .chain(
                items
                    .traits
                    .iter()
                    .map(|(id, _)| (IdKind::Trait, id.into_raw())),
            )
            .chain(
                items
                    .impls
                    .iter()
                    .map(|(id, _)| (IdKind::Impl, id.into_raw())),
            )
            .chain(
                items
                    .consts
                    .iter()
                    .map(|(id, _)| (IdKind::Const, id.into_raw())),
            )
            .chain(
                items
                    .type_aliases
                    .iter()
                    .map(|(id, _)| (IdKind::TypeAlias, id.into_raw())),
            );
        let mut reverse = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        for definition in definitions {
            if let Some(key) = builder.key(definition, 0)
                && reverse.insert(key.clone(), definition).is_some()
            {
                ambiguous.insert(key);
            }
        }
        // A child of a duplicate module/owner is ambiguous even when its own
        // name appears in just one of the duplicate declarations.
        reverse.retain(|key, _| !ambiguous.iter().any(|prefix| key.starts_with(prefix)));
        let keys = reverse.iter().map(|(key, &id)| (id, key.clone())).collect();
        Self {
            keys,
            definitions: reverse,
        }
    }

    /// Returns the edit-stable path, or `None` when identity is unsupported or ambiguous.
    pub fn key(&self, kind: IdKind, raw: u32) -> Option<&[u8]> {
        self.keys.get(&(kind, raw)).map(Vec::as_slice)
    }

    /// Resolves a persisted path in this graph, rejecting removed or ambiguous definitions.
    pub fn lookup(&self, key: &[u8]) -> Option<(IdKind, u32)> {
        self.definitions.get(key).copied()
    }

    /// Encodes a fully normalized concrete type using edit-stable definition paths.
    ///
    /// This identifies a type's structure, not the layout or bodies of referenced
    /// definitions. Query dependencies must track those separately. Anonymous
    /// closure types, unresolved types and missing or ambiguous definitions decline.
    pub fn type_key(&self, canonical_ty: Ty) -> Option<Vec<u8>> {
        if type_identity::identity_postorder(canonical_ty)
            .ok()?
            .iter()
            .any(|ty| matches!(ty.kind(), TyKind::Closure(..)))
        {
            return None;
        }
        let structural = type_identity::stable_key_with(canonical_ty, |kind, raw| {
            self.key(kind, raw)
                .map(<[u8]>::to_vec)
                .ok_or("missing query definition identity")
        })
        .ok()?;
        let mut key = b"flux-query-type-v1".to_vec();
        append(&mut key, &structural);
        Some(key)
    }
}

struct Builder<'a> {
    items: &'a ItemTree,
    namespaces: &'a [(CrateId, &'a [u8])],
    keys: BTreeMap<Definition, Option<Vec<u8>>>,
    headers: BTreeMap<FileId, Vec<(TextRange, Vec<u8>)>>,
}

impl Builder<'_> {
    fn key(&mut self, definition: Definition, depth: usize) -> Option<Vec<u8>> {
        if depth > MAX_IDENTITY_DEPTH {
            return None;
        }
        if let Some(key) = self.keys.get(&definition) {
            return key.clone();
        }
        let key = self.compute(definition, depth + 1);
        self.keys.insert(definition, key.clone());
        key
    }

    fn module(&mut self, module: ModuleId, depth: usize) -> Option<Vec<u8>> {
        self.key((IdKind::Module, module.into_raw()), depth)
    }

    fn owner(&mut self, owner: Owner, depth: usize) -> Option<Vec<u8>> {
        match owner {
            Owner::Impl(id) => self.key((IdKind::Impl, id.into_raw()), depth),
            Owner::Trait(id) => self.key((IdKind::Trait, id.into_raw()), depth),
        }
    }

    fn impl_header(&mut self, file: FileId, self_range: TextRange) -> Option<&[u8]> {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.headers.entry(file) {
            let source = self.items.files.get(&file)?;
            if source.expansion_of.is_some() {
                return None;
            }
            // Scan each file once, rather than traversing its entire syntax
            // tree again for every impl whose associated items need keys.
            let mut headers: Vec<_> = source
                .parse
                .root()
                .descendants()
                .filter(|node| node.kind() == SyntaxKind::IMPL)
                .filter_map(|node| {
                    let body = node
                        .children()
                        .find(|n| n.kind() == SyntaxKind::ASSOC_ITEM_LIST)?;
                    let mut header = Vec::new();
                    for token in node
                        .tokens()
                        .take_while(|t| t.raw_range().end() <= body.raw_range().start())
                    {
                        if token.kind().is_trivia() {
                            continue;
                        }
                        if token.ctxt() != 0 {
                            return None;
                        }
                        append(&mut header, &(token.lexical_kind() as u16).to_le_bytes());
                        append(&mut header, token.text().as_bytes());
                    }
                    Some((
                        TextRange::new(node.text_range().start(), body.text_range().start()),
                        header,
                    ))
                })
                .collect();
            headers.sort_unstable_by_key(|(range, _)| range.start());
            entry.insert(headers);
        }
        let headers = self.headers.get(&file)?;
        let next = headers.partition_point(|(range, _)| range.start() <= self_range.start());
        let (range, header) = headers.get(next.checked_sub(1)?)?;
        contains(*range, self_range).then_some(header.as_slice())
    }

    fn compute(&mut self, (kind, raw): Definition, depth: usize) -> Option<Vec<u8>> {
        let items = self.items;
        let (mut key, name, file) = match kind {
            IdKind::Module => {
                let module = items.modules.get(flux_arena::Idx::from_raw(raw))?;
                let mut key = if let Some(parent) = module.parent {
                    self.module(parent, depth)?
                } else {
                    let mut matching = self.namespaces.iter().filter(|(id, _)| *id == module.krate);
                    let (_, namespace) = matching.next()?;
                    if namespace.is_empty()
                        || matching.next().is_some()
                        || self
                            .namespaces
                            .iter()
                            .any(|(id, value)| id != &module.krate && value == namespace)
                    {
                        return None;
                    }
                    let mut key = b"flux-query-definition-v1".to_vec();
                    append(&mut key, namespace);
                    append(
                        &mut key,
                        format!("{:?}", items.crates.get(module.krate.0 as usize)?.edition)
                            .as_bytes(),
                    );
                    key
                };
                if let Some(block) = module.block {
                    let function = block.function?;
                    let parent = items.functions.get(function)?;
                    if parent.loc.file != module.file {
                        return None;
                    }
                    let source = items.files.get(&module.file)?;
                    let tree = source.parse.tree();
                    // Decoded metadata can retain semantic node IDs while its
                    // source file has only a placeholder syntax tree.
                    if parent.node.index() >= tree.node_count()
                        || block.node.index() >= tree.node_count()
                    {
                        return None;
                    }
                    let owner = tree.node(parent.node);
                    let block = tree.node(block.node);
                    if owner.kind() != SyntaxKind::FN
                        || !contains(owner.raw_range(), block.raw_range())
                    {
                        return None;
                    }
                    append(&mut key, b"body-block");
                    append(
                        &mut key,
                        &self.key((IdKind::Function, function.into_raw()), depth)?,
                    );
                    append(&mut key, &flux_hir::flib::digest(owner.text().as_bytes()));
                    append(
                        &mut key,
                        &(block.raw_range().start() - owner.raw_range().start()).to_le_bytes(),
                    );
                }
                (key, module.name, module.file)
            }
            IdKind::Function => {
                let function = items.functions.get(flux_arena::Idx::from_raw(raw))?;
                if function.derived.is_some() || function.ctor.is_some() {
                    return None;
                }
                let key = match function.owner {
                    Some(owner) => self.owner(owner, depth)?,
                    None => self.module(function.loc.module, depth)?,
                };
                (key, function.name, function.loc.file)
            }
            IdKind::Struct => {
                let item = items.structs.get(flux_arena::Idx::from_raw(raw))?;
                (
                    self.module(item.loc.module, depth)?,
                    item.name,
                    item.loc.file,
                )
            }
            IdKind::Trait => {
                let item = items.traits.get(flux_arena::Idx::from_raw(raw))?;
                (
                    self.module(item.loc.module, depth)?,
                    item.name,
                    item.loc.file,
                )
            }
            IdKind::Const => {
                let item = items.consts.get(flux_arena::Idx::from_raw(raw))?;
                if item.anon.is_some() || item.discriminant.is_some() {
                    return None;
                }
                let key = match item.owner {
                    Some(owner) => self.owner(owner, depth)?,
                    None => self.module(item.loc.module, depth)?,
                };
                (key, item.name?, item.loc.file)
            }
            IdKind::TypeAlias => {
                let item = items.type_aliases.get(flux_arena::Idx::from_raw(raw))?;
                (
                    self.module(item.loc.module, depth)?,
                    item.name,
                    item.loc.file,
                )
            }
            IdKind::Impl => {
                let item = items.impls.get(flux_arena::Idx::from_raw(raw))?;
                let mut key = self.module(item.module, depth)?;
                append(&mut key, b"impl-header");
                key.extend_from_slice(self.impl_header(item.file, item.self_range)?);
                return Some(key);
            }
            _ => return None,
        };
        if items.files.get(&file)?.expansion_of.is_some() {
            return None;
        }
        append(&mut key, &[kind as u8]);
        append(&mut key, name.as_str().as_bytes());
        Some(key)
    }
}

fn append(key: &mut Vec<u8>, value: &[u8]) {
    key.extend_from_slice(&(value.len() as u64).to_le_bytes());
    key.extend_from_slice(value);
}

fn contains(outer: TextRange, inner: TextRange) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_intern::Symbol;

    fn graph(source: &str) -> (ItemTree, CrateId) {
        let mut db = flux_hir::SourceDb::new();
        let root = db.add("/query-test/lib.rs", source);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[flux_hir::SourceCrate {
                root,
                name: Symbol::intern("query_test"),
                dependencies: vec![],
            }],
        );
        let krate = hir.items.main_crate();
        (hir.items, krate)
    }

    fn function_keys(source: &str) -> BTreeMap<String, Vec<u8>> {
        let (items, krate) = graph(source);
        let keys = DefinitionKeys::new(&items, &[(krate, b"test-package|library|target|cfg")]);
        items
            .functions
            .iter()
            .filter(|(_, f)| items.modules[f.loc.module].krate == krate)
            .filter_map(|(id, f)| {
                keys.key(IdKind::Function, id.into_raw())
                    .map(|key| (f.name.to_string(), key.to_vec()))
            })
            .collect()
    }

    #[test]
    fn unrelated_body_edits_positions_and_item_order_preserve_paths() {
        let before =
            function_keys("fn one() -> u8 { 1 } mod inner { pub fn two() {} } fn three() {}");
        let after = function_keys(
            "\n// line shifts\nfn three() { let changed = 55; } fn added() {} mod inner { pub fn two() {} } fn one() -> u8 { 99 }",
        );
        for (name, key) in before {
            assert_eq!(Some(&key), after.get(&name), "identity changed for {name}");
        }
    }

    #[test]
    fn impl_headers_disambiguate_owners_without_hashing_method_bodies() {
        let before = function_keys(
            "struct S; trait First { fn first(); } trait Second { fn second(); } impl First for S { fn first() {} } impl Second for S { fn second() {} }",
        );
        let after = function_keys(
            "struct S; trait Second { fn second(); } trait First { fn first(); } impl /* comment */ Second for S { fn second() { let x = 1; } } impl First for S { fn first() {} }",
        );
        assert_eq!(
            before, after,
            "impl ordering and bodies must not change owner paths"
        );
        let (items, krate) = graph(
            "struct S; trait A { fn f(); } trait B { fn f(); } impl A for S { fn f() {} } impl B for S { fn f() {} }",
        );
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        let distinct: BTreeSet<_> = items
            .functions
            .iter()
            .filter(|(_, f)| f.name.as_str() == "f" && items.modules[f.loc.module].krate == krate)
            .map(|(id, _)| keys.key(IdKind::Function, id.into_raw()).unwrap().to_vec())
            .collect();
        assert_eq!(
            distinct.len(),
            4,
            "trait and impl owners must have distinct paths"
        );
    }

    #[test]
    fn duplicate_owners_decline_their_children() {
        let (items, krate) = graph(
            "struct S; impl S { fn first() {} } impl S { fn second() {} } fn repeated() {} fn repeated() {}",
        );
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        for (id, f) in items
            .functions
            .iter()
            .filter(|(_, f)| items.modules[f.loc.module].krate == krate)
        {
            assert!(
                keys.key(IdKind::Function, id.into_raw()).is_none(),
                "ambiguous owner of {} must decline",
                f.name
            );
        }
    }

    #[test]
    fn const_generic_impl_headers_preserve_token_boundaries() {
        let (items, krate) = graph(
            "struct S<const N: usize>; impl S<{ 1 + 2 }> { fn f() {} } impl S<{ 12 }> { fn f() {} }",
        );
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        let functions: Vec<_> = items
            .functions
            .iter()
            .filter(|(_, f)| f.name.as_str() == "f" && items.modules[f.loc.module].krate == krate)
            .map(|(id, _)| keys.key(IdKind::Function, id.into_raw()).unwrap().to_vec())
            .collect();
        assert_eq!(
            functions.len(),
            2,
            "test must collect both const generic impls"
        );
        assert_ne!(
            functions[0], functions[1],
            "const generic expression tokens are part of owner identity"
        );
    }

    #[test]
    fn decoded_placeholder_trees_decline_nested_owner_identity() {
        let (mut items, krate) = graph("fn outer() { fn nested() {} nested(); }");
        let nested = items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "nested")
            .unwrap()
            .0;
        let file = items.functions[nested].loc.file;
        std::sync::Arc::make_mut(items.files.get_mut(&file).unwrap()).parse =
            flux_syntax::parse_source_file("");
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        assert!(keys.key(IdKind::Function, nested.into_raw()).is_none());
    }

    #[test]
    fn reverse_lookup_remaps_arena_indices_and_rejects_removed_items() {
        let (before, krate) = graph("fn kept() {} fn removed() {}");
        let old = DefinitionKeys::new(&before, &[(krate, b"package")]);
        let (after, new_krate) = graph("fn inserted() {} fn kept() {}");
        let new = DefinitionKeys::new(&after, &[(new_krate, b"package")]);
        let old_id = before
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "kept")
            .unwrap()
            .0;
        let new_id = after
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "kept")
            .unwrap()
            .0;
        assert_ne!(old_id, new_id, "test must shift process arena indices");
        assert_eq!(
            new.lookup(old.key(IdKind::Function, old_id.into_raw()).unwrap()),
            Some((IdKind::Function, new_id.into_raw()))
        );
        let removed = before
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "removed")
            .unwrap()
            .0;
        assert_eq!(
            new.lookup(old.key(IdKind::Function, removed.into_raw()).unwrap()),
            None
        );
    }

    #[test]
    fn namespaces_are_explicit_unique_and_part_of_identity() {
        let (items, krate) = graph("fn f() {}");
        let id = items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "f")
            .unwrap()
            .0
            .into_raw();
        let first = DefinitionKeys::new(&items, &[(krate, b"package|target-a")]);
        let second = DefinitionKeys::new(&items, &[(krate, b"package|target-b")]);
        assert_ne!(
            first.key(IdKind::Function, id),
            second.key(IdKind::Function, id)
        );
        for namespaces in [
            &[][..],
            &[(krate, &b""[..])][..],
            &[(krate, &b"package"[..]), (krate, &b"package"[..])][..],
            &[
                (krate, &b"package"[..]),
                (CrateId(u32::MAX), &b"package"[..]),
            ][..],
        ] {
            assert!(
                DefinitionKeys::new(&items, namespaces)
                    .key(IdKind::Function, id)
                    .is_none(),
                "missing or ambiguous namespaces must decline"
            );
        }
    }

    #[test]
    fn nested_items_follow_their_owner_body_revision() {
        let before = function_keys("fn unrelated() {} fn owner() { fn nested() {} }");
        let shifted = function_keys(
            "\nfn inserted() {} fn owner() { fn nested() {} } fn unrelated() { let changed = 1; }",
        );
        assert_eq!(before.get("nested"), shifted.get("nested"));
        assert!(
            before.contains_key("nested"),
            "test must lower a nested item"
        );
        let changed =
            function_keys("fn owner() { let changed = 1; fn nested() {} } fn unrelated() {}");
        assert_ne!(before.get("nested"), changed.get("nested"));
        assert_eq!(before.get("owner"), changed.get("owner"));
    }

    #[test]
    fn macro_generated_and_anonymous_items_decline() {
        let (items, krate) = graph(
            "macro_rules! make { () => { fn generated() {} } } make!(); const _: u8 = 1; fn physical() {}",
        );
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        let generated = items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "generated")
            .unwrap()
            .0;
        assert!(keys.key(IdKind::Function, generated.into_raw()).is_none());
        let physical = items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "physical")
            .unwrap()
            .0;
        assert!(keys.key(IdKind::Function, physical.into_raw()).is_some());
        for (id, _) in items
            .consts
            .iter()
            .filter(|(_, c)| items.modules[c.loc.module].krate == krate)
        {
            assert!(keys.key(IdKind::Const, id.into_raw()).is_none());
        }
    }

    #[test]
    fn concrete_type_keys_survive_edits_and_retain_all_arguments() {
        fn typed(source: &str, argument: Ty) -> Vec<u8> {
            let (items, krate) = graph(source);
            let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
            let (id, item) = items
                .structs
                .iter()
                .find(|(_, item)| item.name.as_str() == "Container")
                .unwrap();
            keys.type_key(Ty::adt(id, item.name, &[argument])).unwrap()
        }
        let before = typed("struct Container<T>(T); fn unrelated() {}", Ty::BOOL);
        let after = typed(
            "\nstruct Inserted; fn unrelated() { let x = 9; } struct Container<T>(T);",
            Ty::BOOL,
        );
        assert_eq!(before, after);
        assert_ne!(
            before,
            typed("struct Container<T>(T);", Ty::int(crate::IntTy::U8))
        );
        let (items, _) = graph("fn f() {}");
        let keys = DefinitionKeys::new(&items, &[]);
        assert!(
            keys.type_key(Ty::BOOL).is_some(),
            "primitive types need no definition mapping"
        );
        assert_ne!(
            keys.type_key(Ty::BOOL).unwrap(),
            type_identity::stable_key(&items, &[], Ty::BOOL).unwrap(),
            "query and runtime type keys have separate domains"
        );
    }

    #[test]
    fn unresolved_and_anonymous_types_decline_query_identity() {
        let (items, krate) = graph("fn f() {} struct Container<T>(T);");
        let keys = DefinitionKeys::new(&items, &[(krate, b"package")]);
        let function = items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "f")
            .unwrap()
            .0;
        let opaque = Ty::opaque(crate::OpaqueTy {
            function,
            index: 0,
            name: Symbol::intern("impl Sized"),
            args: &[],
            lifetimes: 0,
        });
        assert!(keys.type_key(opaque).is_none());
        assert!(keys.type_key(Ty::ERROR).is_none());
        let closure = Ty::new(TyKind::Closure(
            crate::ClosureId {
                owner: crate::ClosureOwner::Function(function),
                expr: flux_arena::Idx::from_raw(0),
            },
            &[],
        ));
        assert!(keys.type_key(closure).is_none());
        assert!(
            keys.type_key(Ty::tuple(&[Ty::BOOL, closure])).is_none(),
            "anonymous nested types must also decline"
        );
        let (id, item) = items
            .structs
            .iter()
            .find(|(_, item)| item.name.as_str() == "Container")
            .unwrap();
        assert!(
            DefinitionKeys::new(&items, &[])
                .type_key(Ty::adt(id, item.name, &[Ty::BOOL]))
                .is_none()
        );
    }
}
