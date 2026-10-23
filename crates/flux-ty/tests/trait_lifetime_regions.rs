//! Prerequisites for trait lifetimes: retain arguments independently of erased
//! types, without accepting programs whose obligations are not checked yet.
use flux_hir::{Hir, Owner};
use flux_ty::regions::{Outlives, Region};
use flux_ty::{Signatures, Ty};

fn resolve(source: &str) -> (Hir, Signatures, Vec<flux_hir::Diagnostic>) {
    let (db, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (sigs, diagnostics) = flux_ty::signatures(&hir.items);
    (hir, sigs, diagnostics)
}

#[test]
fn ast_node_owner_regions_survive_erasure() {
    let (hir, sigs, diagnostics) = resolve(include_str!("data/trait_lifetime_regions/ast_node.rs"));
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message == "lifetime parameters on traits are not supported yet")
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("undeclared lifetime")),
        "{diagnostics:?}"
    );
    let (trait_id, trait_data) = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "AstNode")
        .unwrap();
    let owner = &sigs.regions.traits[trait_id];
    assert_eq!(owner.names.len(), 1);
    assert_eq!(owner.names[0].name.unwrap().as_str(), "'t");
    for (id, function) in hir
        .items
        .functions
        .iter()
        .filter(|(_, f)| f.owner == Some(Owner::Trait(trait_id)))
    {
        let regions = &sigs.regions.fns[id];
        assert_eq!(regions.owner, 1);
        assert_eq!(regions.names, owner.names);
        match function.name.as_str() {
            "cast" => assert_eq!(&*regions.params[0], &[Region::Param(0)]),
            "syntax" => assert_eq!(&*regions.ret, &[Region::Param(0)]),
            _ => unreachable!(),
        }
    }
    for (id, function) in hir.items.functions.iter().filter(|(_, f)| {
        f.loc.file == trait_data.loc.file
            && f.owner.is_none()
            && matches!(f.name.as_str(), "cast" | "read")
    }) {
        let regions = &sigs.regions.fns[id];
        assert_eq!(
            regions.bound_traits.len(),
            1,
            "{}: {:?} {:?} {:?}",
            function.name,
            function.generics.predicates,
            sigs.fn_predicates.get(id),
            diagnostics
        );
        assert_eq!(&*regions.bound_traits[0].lifetimes, &[Region::Param(0)]);
    }
    let (id, _) = hir
        .items
        .impls
        .iter()
        .find(|(_, i)| {
            i.trait_ref.as_ref().is_some_and(|b| {
                b.path
                    .segments
                    .last()
                    .is_some_and(|s| s.as_str() == "AstNode")
            })
        })
        .unwrap();
    assert_eq!(
        &*sigs.regions.impls[id].trait_lifetimes,
        &[Region::Param(0)]
    );
}

#[test]
fn owner_and_method_parameters_have_distinct_slots_and_bounds() {
    let (hir, sigs, diagnostics) = resolve(
        "\
        trait Pair<'a, 'b: 'a, T: 'b> where Self: 'a {\n\
            fn pick<'c: 'a>(x: &'b T, y: &'c T) -> &'a T;\n\
        }\n\
        fn main() {}\n",
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.message.contains("undeclared lifetime")),
        "{diagnostics:?}"
    );
    let (id, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "pick")
        .unwrap();
    let regions = &sigs.regions.fns[id];
    assert_eq!(regions.owner, 2);
    assert_eq!(regions.names.len(), 3);
    assert_eq!(&*regions.params[0], &[Region::Param(1)]);
    assert_eq!(&*regions.params[1], &[Region::Param(2)]);
    assert_eq!(&*regions.ret, &[Region::Param(0)]);
    assert!(
        regions
            .outlives
            .contains(&Outlives::Region(Region::Param(1), Region::Param(0)))
    );
    assert!(
        regions
            .outlives
            .contains(&Outlives::Region(Region::Param(2), Region::Param(0)))
    );
    let self_ty = Ty::param(0, flux_intern::kw::SelfUpper);
    assert!(
        regions
            .outlives
            .contains(&Outlives::Type(self_ty, Region::Param(0)))
    );
    let t = Ty::param(1, flux_intern::Symbol::intern("T")); // Self occupies type slot zero.
    assert!(
        regions
            .outlives
            .contains(&Outlives::Type(t, Region::Param(1)))
    );
}

#[test]
fn repeated_static_and_type_argument_regions_are_separate() {
    let (hir, sigs, _) = resolve(
        "\
        trait Tr<'a, 'b, T> { type Out; }\n\
        struct Node<'a>(&'a i32);\n\
        impl<'x> Tr<'x, 'x, &'static i32> for Node<'x> { type Out = &'x i32; }\n\
        fn bound<'a, N: Tr<'static, 'a, &'a i32>>() {}\n\
        fn main() {}\n",
    );
    let (id, _) = hir
        .items
        .impls
        .iter()
        .find(|(_, i)| {
            i.trait_ref
                .as_ref()
                .is_some_and(|b| b.path.segments.last().is_some_and(|s| s.as_str() == "Tr"))
        })
        .unwrap();
    let regions = &sigs.regions.impls[id];
    assert_eq!(
        &*regions.trait_lifetimes,
        &[Region::Param(0), Region::Param(0)]
    );
    assert_eq!(&*regions.trait_args[0], &[Region::Static]);
    assert_eq!(&*regions.self_ty, &[Region::Param(0)]);
    assert_eq!(&*regions.types[0].1, &[Region::Param(0)]);
    let (id, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "bound")
        .unwrap();
    let table = &sigs.regions.fns[id].bound_traits[0];
    assert_eq!(&*table.lifetimes, &[Region::Static, Region::Param(0)]);
    assert_eq!(&*table.args[0], &[Region::Param(0)]);
}

#[test]
fn extended_signatures_keep_region_metadata_consistent() {
    for source in [
        include_str!("data/trait_lifetime_regions/ast_node.rs"),
        "trait Tr<'a, 'b: 'a> { fn get(&'b self) -> &'a Self; } fn main() {}",
    ] {
        let (hir, shared, mut diagnostics) = resolve(source);
        let (all, mut all_diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
        assert_eq!(format!("{shared:?}"), format!("{all:?}"));
        flux_hir::Diagnostic::sort(&mut diagnostics);
        flux_hir::Diagnostic::sort(&mut all_diagnostics);
        assert_eq!(diagnostics, all_diagnostics);
    }
}

#[test]
fn trait_arguments_constrain_associated_type_lifetimes() {
    for (argument, unconstrained) in [("'a", false), ("'static", true)] {
        let source = format!(
            "trait Tr<'a> {{ type Out; }} struct S; impl<'a> Tr<{argument}> for S {{ type Out = &'a i32; }} fn main() {{}}"
        );
        let (_, _, diagnostics) = resolve(&source);
        assert_eq!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("is not constrained by the impl trait")),
            unconstrained,
            "{source}: {diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message == "lifetime parameters on traits are not supported yet")
        );
    }
}
