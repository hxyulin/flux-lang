//! Family-local GAT syntax is retained for later type and region checking.
//! HIR admission alone does not prove a family implementation is valid.
use flux_hir::{ParamKind, TypeRef};
use std::process::Command;

#[test]
fn family_metadata_keeps_own_binders_bounds_and_both_where_locations() {
    let source = "trait Family<'owner> {type Item<'a,T,const N:usize>: Copy+'a where Self:'a,'owner:'a,T:Copy+'a;}struct Host;impl<'owner> Family<'owner> for Host {type Item<'a,T,const N:usize> where T:Copy = &'a[T;N] where Self:'a,'owner:'a,T:'a;}";
    let (db, file, hir) = flux_hir::lower_text("gat-metadata.rs", source);
    assert!(db.syntax_errors().is_empty());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let family = hir
        .items
        .traits
        .iter()
        .find(|(_, tr)| tr.name.as_str() == "Family")
        .unwrap()
        .1;
    assert_eq!(family.generics.lifetimes.len(), 1);
    assert_eq!(family.generics.lifetimes[0].name.as_str(), "'owner");
    let item = &family.types[0];
    assert_eq!(item.generics.lifetimes.len(), 1);
    assert_eq!(item.generics.lifetimes[0].name.as_str(), "'a");
    assert_eq!(item.generics.params.len(), 2);
    assert_eq!(item.generics.params[0].name.as_str(), "T");
    assert!(matches!(item.generics.params[0].kind, ParamKind::Type));
    assert_eq!(item.generics.params[1].name.as_str(), "N");
    assert!(matches!(
        item.generics.params[1].kind,
        ParamKind::Const { .. }
    ));
    assert_eq!(item.bounds[0].path.as_name().unwrap().as_str(), "Copy");
    assert_eq!(item.lifetime_bounds.len(), 1);
    assert_eq!(item.lifetime_bounds[0].name.as_str(), "'a");
    assert_eq!(item.generics.outlives.len(), 1);
    assert_eq!(item.generics.outlives[0].longer.name.as_str(), "'owner");
    assert_eq!(item.generics.outlives[0].shorter.name.as_str(), "'a");
    assert_eq!(item.generics.type_outlives.len(), 2);
    for predicate in &item.generics.type_outlives {
        assert_eq!(predicate.lifetime.name.as_str(), "'a");
        assert_eq!(
            &source[predicate.lifetime.range.start() as usize
                ..predicate.lifetime.range.end() as usize],
            "'a"
        );
    }
    let implementation = hir
        .items
        .impls
        .iter()
        .find(|(_, imp)| imp.file == file)
        .unwrap()
        .1;
    assert_eq!(implementation.generics.lifetimes.len(), 1);
    let defined = &implementation.types[0];
    assert_eq!(defined.generics.lifetimes.len(), 1);
    assert_eq!(defined.generics.params.len(), 2);
    assert_eq!(defined.generics.predicates.len(), 1);
    assert_eq!(defined.generics.outlives.len(), 1);
    assert_eq!(defined.generics.type_outlives.len(), 2);
    assert!(
        matches!(&defined.ty, TypeRef::Ref { lifetime: Some(lifetime), pointee, .. } if lifetime.name.as_str() == "'a" && matches!(pointee.as_ref(), TypeRef::Array { .. }))
    );
    assert_eq!(
        &source[defined.ty_range.start() as usize..defined.ty_range.end() as usize],
        "&'a[T;N]"
    );
}

#[test]
fn associated_family_syntax_matches_rustc_at_the_hir_boundary() {
    let directory = std::env::temp_dir().join(format!("flux-gat-hir-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepted) in [
        (
            "lifetime",
            "trait A{type Item<'a> where Self:'a;}struct S;impl A for S{type Item<'a>=&'a u32 where Self:'a;}",
            true,
        ),
        (
            "type",
            "trait A{type Item<T>;}struct S;impl A for S{type Item<U>=Vec<U>;}",
            true,
        ),
        (
            "const",
            "trait A{type Item<const N:usize>;}struct S;impl A for S{type Item<const M:usize>=[u8;M];}",
            true,
        ),
        (
            "mixed",
            "trait A{type Item<'a,T:Copy+'a,const N:usize>:Copy+'a;}struct S;impl A for S{type Item<'b,U:Copy+'b,const M:usize>=&'b[U;M];}",
            true,
        ),
        (
            "where_after",
            "trait A{type Item<'a,T> where T:'a;}struct S;impl A for S{type Item<'a,T>=&'a T where T:'a;}",
            true,
        ),
        (
            "where_both",
            "trait A{type Item<'a,T> where Self:'a,T:Copy+'a;}struct S;impl A for S{type Item<'a,T> where T:Copy = &'a T where Self:'a,T:'a;}",
            true,
        ),
        (
            "iterator_equality",
            "trait A{type Item<'a> where Self:'a;type Iter<'a>:Iterator<Item=Self::Item<'a>> where Self:'a;}",
            true,
        ),
        (
            "independent_owner",
            "trait A<'data>{type Item<'file>:'file where Self:'file,'data:'file;}",
            true,
        ),
        ("default_type", "trait A{type Item<T=u8>;}", false),
        (
            "default_const",
            "trait A{type Item<const N:usize=1>;}",
            false,
        ),
        (
            "default_associated",
            "trait A{type Item<'a>=&'a u8;}",
            false,
        ),
        (
            "impl_default_type",
            "trait A{type Item<T>;}struct S;impl A for S{type Item<T=u8>=T;}",
            false,
        ),
        (
            "impl_default_const",
            "trait A{type Item<const N:usize>;}struct S;impl A for S{type Item<const N:usize=1>=[u8;N];}",
            false,
        ),
        ("inherent", "struct S;impl S{type Item<'a>=&'a u8;}", false),
        (
            "missing_rhs",
            "trait A{type Item<'a>;}struct S;impl A for S{type Item<'a>;}",
            false,
        ),
        ("duplicate_param", "trait A{type Item<T,T>;}", false),
        (
            "duplicate_family",
            "trait A{type Item<'a>;type Item<'b>;}",
            false,
        ),
    ] {
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-o",
            ])
            .arg(directory.join("probe.rmeta"))
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let clean = db.syntax_errors().is_empty() && hir.diagnostics.is_empty();
        assert_eq!(
            clean,
            accepted,
            "HIR {name}: {:?} {:?}",
            db.syntax_errors(),
            hir.diagnostics
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unrepresented_family_shapes_keep_explicit_hir_gates() {
    for (source, gate) in [
        (
            "trait A{type Item<'a>:?Sized;}",
            "`?Sized` bounds are not supported yet",
        ),
        (
            "trait A{type Item<'a>;}fn f<T:A<Item<'static>=u8>>(){}",
            "generic associated types are not supported yet",
        ),
    ] {
        let (_, _, hir) = flux_hir::lower_text("gat-gate.rs", source);
        assert!(
            hir.diagnostics.iter().any(|d| d.message == gate),
            "{:?}",
            hir.diagnostics
        );
    }
}
