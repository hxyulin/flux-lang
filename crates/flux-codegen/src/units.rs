//! Ownership-preserving partitions for the opt-in fast-build backend.

use std::num::NonZeroUsize;

use keel_ir::{CallConv, DataItem, DataObject, FuncDecl, FunctionAttrs, Module};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Error, Stage};

/// Splits reified functions into deterministic native compilation units.
///
/// Each definition occurs in one unit; other units borrow declarations for the
/// symbols they reference. Private definitions become hidden symbols so
/// references can cross object boundaries. Unit assignment
/// hashes existing symbol names, so this alone does not make those names stable
/// across source edits. Private read-only data used exclusively by one function
/// unit lives with that unit, including private constant initializer chains.
///
/// A count of one returns the original module. Module assembly and debug
/// metadata also retain one module. Lifecycle registrations and explicitly
/// placed functions or data retain one module to preserve their order. Callers
/// must retain the ordinary single-module path for these cases.
///
/// # Errors
/// Returns a verification error for invalid partitioned IR or for `cc(fast)`
/// signatures, which require definitions local to one module. Callers must
/// lower through the reified ABI rather than changing the calling convention
/// on already lowered functions.
pub fn split_fast_module(module: Module, count: NonZeroUsize) -> Result<Vec<Module>, Error> {
    if count.get() == 1
        || !module.assembly.is_empty()
        || !module.files.is_empty()
        || !module.debug_types.is_empty()
        || !module.debug_globals.is_empty()
        || module.debug_language.is_some()
        || module.functions.iter().any(|function| {
            function.attrs.constructor
                || function.attrs.destructor
                || function.attrs.section.is_some()
        })
        || module.data.iter().any(|object| object.section.is_some())
    {
        return Ok(vec![module]);
    }
    verify(&module)?;
    if module.functions.iter().any(|f| f.sig.cc == CallConv::Fast)
        || module.decls.iter().any(|f| f.sig.cc == CallConv::Fast)
    {
        return Err(Error::message(
            Stage::Verify,
            "backend units require reified function signatures",
        ));
    }

    let Module {
        decls,
        data,
        functions,
        ..
    } = module;
    let data_owners = data_owners(&functions, &data, count);
    let declarations: Vec<_> = functions
        .iter()
        .map(|function| FuncDecl {
            name: function.name.clone(),
            sig: function.sig.clone(),
            attrs: declaration_attrs(function.attrs.clone()),
        })
        .collect();
    let data_declarations: Vec<_> = data
        .iter()
        .zip(&data_owners)
        .filter(|(object, _)| object.init.is_some())
        .map(|(object, &owner)| (owner, data_declaration(object)))
        .collect();
    let external_data: Vec<_> = data
        .iter()
        .filter(|object| object.init.is_none())
        .cloned()
        .collect();
    let mut initializer_references: Vec<FxHashSet<&str>> =
        (0..count.get()).map(|_| FxHashSet::default()).collect();
    for (object, &owner) in data.iter().zip(&data_owners) {
        if let Some(init) = &object.init {
            initializer_references[owner].extend(init.items.iter().filter_map(|item| match item {
                DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } => {
                    Some(symbol.as_str())
                }
                _ => None,
            }));
        }
    }
    let mut units: Vec<_> = (0..count.get()).map(|_| Module::default()).collect();
    for mut function in functions {
        let owner = owner(&function.name, count);
        if function.local {
            function.local = false;
            function.hidden = true;
            function.attrs.noninterposable = true;
        }
        units[owner].functions.push(function);
    }
    for (index, unit) in units.iter_mut().enumerate() {
        let mut references: FxHashSet<_> = unit
            .functions
            .iter()
            .flat_map(|function| {
                function
                    .callees()
                    .map(|(_, callee)| callee.name.as_str())
                    .chain(function.data_symbols().map(|(_, name)| name))
                    .chain(function.personality.as_deref())
            })
            .collect();
        references.extend(initializer_references[index].iter().copied());
        // Keep producer-declared externals: local optimization may introduce
        // runtime calls even when the initial function tables do not name them.
        unit.decls.clone_from(&decls);
        unit.decls.extend(
            declarations
                .iter()
                .filter(|d| owner(&d.name, count) != index && references.contains(d.name.as_str()))
                .cloned(),
        );
        unit.data.clone_from(&external_data);
        unit.data.extend(
            data_declarations
                .iter()
                .filter(|(owner, d)| *owner != index && references.contains(d.name.as_str()))
                .map(|(_, d)| d.clone()),
        );
    }
    drop(initializer_references);
    for (mut object, owner) in data
        .into_iter()
        .zip(data_owners)
        .filter(|(object, _)| object.init.is_some())
    {
        if object.local {
            object.local = false;
            object.hidden = true;
        }
        units[owner].data.push(object);
    }
    for unit in &mut units {
        unit.functions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        unit.decls.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        unit.data.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        unit.sync_function_attrs();
        verify(unit)?;
    }
    Ok(units)
}

fn data_owners(
    functions: &[keel_ir::Function],
    data: &[DataObject],
    count: NonZeroUsize,
) -> Vec<usize> {
    let indices: FxHashMap<_, _> = data
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    let mut consumers: Vec<_> = data
        .iter()
        .map(|object| {
            if object.local
                && !object.thread_local
                && object.init.as_ref().is_some_and(|init| init.constant)
            {
                DataConsumers::Unused
            } else {
                DataConsumers::Shared
            }
        })
        .collect();
    for function in functions {
        let unit = owner(&function.name, count);
        for (_, name) in function.data_symbols() {
            if let Some(&index) = indices.get(name) {
                consumers[index] = consumers[index].merge(DataConsumers::Unit(unit));
            }
        }
    }
    let mut offsets = vec![0];
    let mut edges = Vec::new();
    for object in data {
        if let Some(init) = &object.init {
            for item in &init.items {
                if let DataItem::Ptr { symbol, .. } | DataItem::RttiName { symbol } = item
                    && let Some(&index) = indices.get(symbol.as_str())
                {
                    edges.push(index);
                }
            }
        }
        offsets.push(edges.len());
    }
    let mut pending: Vec<_> = consumers
        .iter()
        .enumerate()
        .filter(|(_, consumer)| **consumer != DataConsumers::Unused)
        .map(|(index, _)| index)
        .collect();
    // Consumer information flows through initializer edges, including cycles.
    // Each node changes at most twice: unused -> one unit -> shared. Unreached
    // data can have non-function roots, so pin it and its descendants afterward.
    for pin_unused in [false, true] {
        if pin_unused {
            for (index, consumer) in consumers.iter_mut().enumerate() {
                if *consumer == DataConsumers::Unused {
                    *consumer = DataConsumers::Shared;
                    pending.push(index);
                }
            }
        }
        while let Some(index) = pending.pop() {
            for &target in &edges[offsets[index]..offsets[index + 1]] {
                let merged = consumers[target].merge(consumers[index]);
                if merged != consumers[target] {
                    consumers[target] = merged;
                    pending.push(target);
                }
            }
        }
    }
    data.iter()
        .zip(consumers)
        .map(|(object, consumer)| match consumer {
            DataConsumers::Unit(unit) => unit,
            _ => owner(&object.name, count),
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DataConsumers {
    Unused,
    Unit(usize),
    Shared,
}

impl DataConsumers {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unused, other) => other,
            (this, Self::Unused) => this,
            (Self::Unit(a), Self::Unit(b)) if a == b => self,
            _ => Self::Shared,
        }
    }
}

fn owner(name: &str, count: NonZeroUsize) -> usize {
    // FNV-1a keeps assignment independent of process hash seeds and worker count.
    let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    (hash % count.get() as u64) as usize
}

fn declaration_attrs(mut attrs: FunctionAttrs) -> FunctionAttrs {
    attrs.constructor = false;
    attrs.destructor = false;
    attrs.retain = false;
    attrs.section = None;
    attrs
}

fn data_declaration(object: &DataObject) -> DataObject {
    DataObject {
        name: object.name.clone(),
        thread_local: object.thread_local,
        weak: object.weak,
        ..DataObject::default()
    }
}

fn verify(module: &Module) -> Result<(), Error> {
    keel_ir::verify_module(module).map_err(|errors| {
        Error::message(
            Stage::Verify,
            format!(
                "invalid backend unit IR: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }

    fn owners(units: &[Module]) -> std::collections::BTreeMap<&str, usize> {
        units
            .iter()
            .enumerate()
            .flat_map(|(index, unit)| unit.functions.iter().map(move |f| (f.name.as_str(), index)))
            .collect()
    }

    #[test]
    fn definitions_have_one_owner_and_cross_unit_references_verify() {
        let mut module = keel_ir::parse(
            "data @value = local align 8 { i64 7 }\n\
             data @address = local const align 8 { ptr @leaf, ptr @value, rtti_name @value }\n\
             func @leaf() -> i64 local {\nblock0:\n v0 = symbol_addr @value\n v1 = load.i64 v0\n return v1\n}\n\
             func @entry() -> i64 {\nblock0:\n v0 = call @leaf()\n return v0\n}\n",
        )
        .unwrap();
        module.sync_function_attrs();
        let original_names: std::collections::BTreeSet<_> =
            module.functions.iter().map(|f| f.name.clone()).collect();
        let units = split_fast_module(module, count(8)).unwrap();
        let names: Vec<_> = units
            .iter()
            .flat_map(|u| &u.functions)
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(names.len(), original_names.len());
        assert_eq!(
            names.into_iter().collect::<std::collections::BTreeSet<_>>(),
            original_names
        );
        assert_ne!(owners(&units)["leaf"], owners(&units)["entry"]);
        for (index, unit) in units.iter().enumerate() {
            keel_ir::verify_module(unit).unwrap();
            for object in &unit.data {
                assert_eq!(
                    object.init.is_some(),
                    index == owner(&object.name, count(8))
                );
                if object.init.is_some() {
                    assert!(!object.local && object.hidden);
                }
            }
            for function in &unit.functions {
                if function.name == "leaf" {
                    assert!(!function.local && function.hidden);
                }
            }
        }
        let address_owner = owner("address", count(8));
        let value_owner = owner("value", count(8));
        assert_ne!(address_owner, value_owner);
        assert!(
            units[address_owner]
                .data
                .iter()
                .any(|d| d.name == "value" && d.init.is_none())
        );
        assert!(units[address_owner].decls.iter().any(|d| d.name == "leaf"));
    }

    #[test]
    fn lifecycle_registrations_keep_original_order() {
        let mut module = keel_ir::parse(
            "func @start() {\nblock0:\n return\n}\n\
             func @stop() {\nblock0:\n return\n}\n",
        )
        .unwrap();
        module.functions[0].attrs.constructor = true;
        module.functions[0].attrs.retain = true;
        module.functions[1].attrs.destructor = true;
        let original = module.to_string();
        let units = split_fast_module(module, count(4)).unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].to_string(), original);
    }

    #[test]
    fn name_assignment_survives_definition_reordering_and_insertion() {
        let module = keel_ir::parse(
            "func @alpha() {\nblock0:\n return\n}\n\
             func @beta() {\nblock0:\n return\n}\n",
        )
        .unwrap();
        let first = split_fast_module(module.clone(), count(4)).unwrap();
        let mut changed = module;
        changed.functions.reverse();
        changed.functions.push(
            keel_ir::parse("func @new() {\nblock0:\n return\n}\n")
                .unwrap()
                .functions
                .remove(0),
        );
        let second = split_fast_module(changed, count(4)).unwrap();
        for (name, owner) in owners(&first) {
            assert_eq!(owners(&second)[name], owner, "{name}");
        }
    }

    #[test]
    fn unrelated_revision_names_do_not_change_an_unreferencing_unit() {
        let count = count(8);
        let entry_owner = owner("entry", count);
        let unrelated: Vec<_> = (0..)
            .map(|i| format!("unrelated_{i}"))
            .filter(|name| owner(name, count) != entry_owner)
            .take(2)
            .collect();
        let tables: Vec<_> = (0..)
            .map(|i| format!("table_{i}"))
            .filter(|name| owner(name, count) != entry_owner)
            .take(2)
            .collect();
        let graph = |revision| {
            let source = format!(
                "data @{} = local const align 8 {{ ptr @{} }}\n\
                 func @{}() {{\nblock0:\n return\n}}\n\
                 func @entry() -> i64 {{\nblock0:\n v0 = iconst.i64 7\n return v0\n}}\n",
                tables[revision], unrelated[revision], unrelated[revision],
            );
            split_fast_module(keel_ir::parse(&source).unwrap(), count).unwrap()
        };
        let first = graph(0);
        let second = graph(1);
        assert_eq!(
            first[entry_owner].to_string(),
            second[entry_owner].to_string()
        );
        assert!(first[entry_owner].decls.is_empty());
        assert!(first[entry_owner].data.is_empty());
        // The initializer's function pointer still needs a declaration in its
        // data-owning unit when that function belongs to a different unit.
        for (revision, units) in [first, second].iter().enumerate() {
            let table_owner = owner(&tables[revision], count);
            if owner(&unrelated[revision], count) != table_owner {
                assert!(
                    units[table_owner]
                        .decls
                        .iter()
                        .any(|d| d.name == unrelated[revision])
                );
            }
        }
    }

    #[test]
    fn traversal_order_does_not_change_partition_contents() {
        let source = "decl @external()\n\
             decl @other()\n\
             decl data @external_data\n\
             decl data thread_local @tls\n\
             data @value = local align 8 { i64 7 }\n\
             data @address = local const align 8 { ptr @leaf, ptr @value }\n\
             func @leaf() -> i64 local {\nblock0:\n v0 = symbol_addr @value\n v1 = load.i64 v0\n return v1\n}\n\
             func @entry() -> i64 {\nblock0:\n v0 = call @leaf()\n return v0\n}\n";
        let module = keel_ir::parse(source).unwrap();
        let first = split_fast_module(module.clone(), count(4)).unwrap();
        let mut reversed = module;
        reversed.functions.reverse();
        reversed.decls.reverse();
        reversed.data.reverse();
        let second = split_fast_module(reversed, count(4)).unwrap();
        for (first, second) in first.iter().zip(&second) {
            assert_eq!(first.to_string(), second.to_string());
            assert!(
                first
                    .data
                    .iter()
                    .any(|d| d.name == "external_data" && d.init.is_none())
            );
            assert!(
                first
                    .data
                    .iter()
                    .any(|d| d.name == "tls" && d.thread_local && d.init.is_none())
            );
        }
    }

    #[test]
    fn private_constant_edit_changes_only_its_consuming_unit() {
        let count = count(8);
        let consumer = owner("consumer", count);
        let names: Vec<_> = (0..)
            .map(|i| format!("literal_{i}"))
            .filter(|name| owner(name, count) != consumer)
            .take(16)
            .collect();
        let old = &names[0];
        let new = names
            .iter()
            .find(|name| owner(name, count) != owner(old, count))
            .unwrap();
        let graph = |name: &str, value| {
            let source = format!(
                "data @{name} = local const align 8 {{ i64 {value} }}\n\
                 func @consumer() -> i64 {{\nblock0:\n v0 = symbol_addr @{name}\n v1 = load.i64 v0\n return v1\n}}\n"
            );
            split_fast_module(keel_ir::parse(&source).unwrap(), count).unwrap()
        };
        let before = graph(old, 7);
        let after = graph(new, 9);
        let changed: Vec<_> = before
            .iter()
            .zip(&after)
            .enumerate()
            .filter(|(_, (a, b))| a.to_string() != b.to_string())
            .map(|(index, _)| index)
            .collect();
        assert_eq!(changed, vec![consumer]);
        assert!(before[consumer].data.iter().any(|data| data.init.is_some()));
        assert!(after[consumer].data.iter().any(|data| data.init.is_some()));
    }

    #[test]
    fn shared_mutable_and_initializer_referenced_data_keep_symbol_assignment() {
        let count = count(8);
        let module = keel_ir::parse(
            "data @shared = local const align 8 { i64 7 }\n\
             data @mutable = local align 8 { i64 7 }\n\
             data @indirect = local const align 8 { i64 7 }\n\
             data @address = local const align 8 { ptr @indirect }\n\
             func @alpha() {\nblock0:\n v0 = symbol_addr @shared\n v1 = symbol_addr @mutable\n v2 = symbol_addr @indirect\n return\n}\n\
             func @beta() {\nblock0:\n v0 = symbol_addr @shared\n return\n}\n",
        )
        .unwrap();
        assert_ne!(owner("alpha", count), owner("beta", count));
        let units = split_fast_module(module, count).unwrap();
        for (index, unit) in units.iter().enumerate() {
            keel_ir::verify_module(unit).unwrap();
            for data in unit.data.iter().filter(|data| data.init.is_some()) {
                assert_eq!(index, owner(&data.name, count), "{}", data.name);
            }
        }
    }

    #[test]
    fn private_initializer_chains_and_cycles_follow_their_consumer() {
        let count = count(8);
        let consumer = owner("consumer", count);
        let module = keel_ir::parse(
            "data @leaf = local const align 8 { i64 7 }\n\
             data @middle = local const align 8 { ptr @leaf }\n\
             data @cycle_a = local const align 8 { ptr @cycle_b }\n\
             data @cycle_b = local const align 8 { ptr @cycle_a }\n\
             data @root = local const align 8 { ptr @middle, ptr @cycle_a }\n\
             func @consumer() {\nblock0:\n v0 = symbol_addr @root\n return\n}\n",
        )
        .unwrap();
        let units = split_fast_module(module.clone(), count).unwrap();
        let mut reordered = module.clone();
        reordered.data.reverse();
        let reordered = split_fast_module(reordered, count).unwrap();
        for (before, after) in units.iter().zip(&reordered) {
            assert_eq!(before.to_string(), after.to_string());
        }
        assert_eq!(
            units[consumer]
                .data
                .iter()
                .filter(|data| data.init.is_some())
                .count(),
            5
        );
        let other = (0..)
            .map(|i| format!("other_{i}"))
            .find(|name| owner(name, count) != consumer)
            .unwrap();
        let mut shared = module;
        shared.functions.push(
            keel_ir::parse(&format!(
                "decl data @cycle_b\nfunc @{other}() {{\nblock0:\n v0 = symbol_addr @cycle_b\n return\n}}\n"
            ))
            .unwrap()
            .functions
            .remove(0),
        );
        let units = split_fast_module(shared, count).unwrap();
        for (index, unit) in units.iter().enumerate() {
            keel_ir::verify_module(unit).unwrap();
            for data in unit.data.iter().filter(|data| data.init.is_some()) {
                let expected = if data.name.starts_with("cycle_") {
                    owner(&data.name, count)
                } else {
                    consumer
                };
                assert_eq!(index, expected, "{}", data.name);
            }
        }
    }

    #[test]
    fn explicit_sections_keep_original_order() {
        let module = keel_ir::parse(
            "data @second = local align 8 { i64 7 }\n\
             data @first = local align 8 { i64 8 }\n\
             func @second_function() {\nblock0:\n return\n}\n\
             func @first_function() {\nblock0:\n return\n}\n",
        )
        .unwrap();
        for function_section in [false, true] {
            let mut placed = module.clone();
            let section = Some(keel_ir::SectionName::new(".custom").unwrap());
            if function_section {
                placed.functions[0].attrs.section = section;
            } else {
                placed.data[0].section = section;
            }
            let original = placed.to_string();
            let units = split_fast_module(placed, count(8)).unwrap();
            assert_eq!(units.len(), 1);
            assert_eq!(units[0].to_string(), original);
        }
    }

    #[test]
    fn fast_abi_is_rejected_without_rewriting_it() {
        let module =
            keel_ir::parse("func @local() cc(fast) local {\nblock0:\n return\n}\n").unwrap();
        assert!(
            split_fast_module(module.clone(), count(2))
                .unwrap_err()
                .to_string()
                .contains("reified")
        );
        assert_eq!(split_fast_module(module, count(1)).unwrap().len(), 1);
    }

    #[test]
    fn opaque_assembly_and_debug_context_keep_the_original_module() {
        let mut module = keel_ir::parse("func @entry() {\nblock0:\n return\n}\n").unwrap();
        module.assembly.push(".Lprivate: nop".into());
        let original = module.to_string();
        let units = split_fast_module(module, count(4)).unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].to_string(), original);
        let mut debug = units.into_iter().next().unwrap();
        debug.assembly.clear();
        debug.files.push("source.rs".into());
        assert_eq!(split_fast_module(debug, count(4)).unwrap().len(), 1);
    }
}
