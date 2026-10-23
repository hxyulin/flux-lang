//! Signature checks exercised below the global trait-lifetime declaration gate.
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use flux_ty::regions::{Outlives, Region, trait_requirements};

fn resolve(source: &str) -> (flux_hir::Hir, flux_ty::Signatures, Vec<String>) {
    let (db, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (sigs, diagnostics) = flux_ty::signatures(&hir.items);
    let messages = diagnostics.into_iter().map(|d| d.message).collect();
    (hir, sigs, messages)
}

fn oracle(source: &str, accepted: bool) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "flux-trait-signature-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("main.rs");
    let exe = dir.join("main");
    std::fs::write(&path, source).unwrap();
    let result = Command::new("rustc")
        .args(["--edition=2024", "-Awarnings"])
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert_eq!(
        result.status.success(),
        accepted,
        "{source}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    if accepted {
        assert!(Command::new(&exe).status().unwrap().success());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn declaration_arity_names_and_elision_match_rustc() {
    for (body, accepted, diagnostic) in [
        ("fn f<'x, N: Tr<'x>>() {}", true, ""),
        ("fn f<N: Tr<'static>>() {}", true, ""),
        ("impl<'x> Tr<'x> for S {}", true, ""),
        ("impl Tr<'_> for S {}", true, ""),
        ("fn f<N: Tr>() {}", false, "missing lifetime specifier"),
        (
            "impl Tr for S {}",
            false,
            "implicit elided lifetime not allowed here",
        ),
        ("fn f<N: Tr<'_>>() {}", false, "cannot be used here"),
        ("fn f<N: Tr<'missing>>() {}", false, "undeclared lifetime"),
        ("impl Tr<'missing> for S {}", false, "undeclared lifetime"),
        (
            "fn f<N: Tr<'static, 'static>>() {}",
            false,
            "trait takes 1 lifetime argument but 2",
        ),
        (
            "impl Tr<'static, 'static> for S {}",
            false,
            "trait takes 1 lifetime argument but 2",
        ),
    ] {
        let source = format!("trait Tr<'a> {{}} struct S; {body} fn main() {{}}");
        oracle(&source, accepted);
        let (_, _, messages) = resolve(&source);
        assert!(
            messages
                .iter()
                .any(|m| m == "lifetime parameters on traits are not supported yet")
        );
        if accepted {
            assert_eq!(
                messages,
                ["lifetime parameters on traits are not supported yet"],
                "{source}"
            );
        } else {
            assert!(
                messages.iter().any(|m| m.contains(diagnostic)),
                "{source}: {messages:?}"
            );
        }
    }
    let source = "trait Tr {} fn f<N: Tr<'static>>() {} fn main() {}";
    oracle(source, false);
    assert!(
        resolve(source)
            .2
            .iter()
            .any(|m| m.contains("trait takes 0 lifetime arguments"))
    );
}

#[test]
fn required_outlives_are_checked_not_assumed() {
    for (trait_decl, argument, requirement) in [
        ("trait Tr<'a, 'b: 'a> {}", "'a, 'b", "'b: 'a"),
        ("trait Tr<'a, 'b: 'a> {}", "'static, 'b", "'b: 'static"),
        ("trait Tr<'a, T: 'a> {}", "'a, &'b i32", "'b: 'a"),
        ("trait Tr<T> {}", "&'a &'b i32", "'b: 'a"),
        ("trait Tr<'a, T: 'a> {}", "'a, T", "T: 'a"),
    ] {
        for bound in [false, true] {
            for header in [false, true] {
                let clause = if bound {
                    format!("where {requirement}")
                } else {
                    String::new()
                };
                let item = if header {
                    format!("impl<'a, 'b, T> Tr<{argument}> for S<T> {clause} {{}}")
                } else {
                    format!("fn f<'a, 'b, T, N: Tr<{argument}>>() {clause} {{}}")
                };
                let source = format!("{trait_decl} struct S<T>(T); {item} fn main() {{}}");
                let accepted = bound || header && trait_decl == "trait Tr<T> {}";
                oracle(&source, accepted);
                let (_, _, messages) = resolve(&source);
                let failed = messages.iter().any(|m| {
                    m == "lifetime bound not satisfied"
                        || m == "type does not fulfill the required lifetime"
                });
                assert_eq!(failed, !accepted, "{source}: {messages:?}");
            }
        }
    }
}

#[test]
fn helper_maps_repeated_static_and_nested_type_regions() {
    let source = "trait Tr<'a, 'b: 'a, T: 'b> {} fn f<'x, 'y, T: 'y, N: Tr<'x, 'x, (&'y T, &'static i32)>>() where 'y: 'x {} fn main() {}";
    oracle(source, true);
    let (hir, sigs, messages) = resolve(source);
    assert!(
        !messages.iter().any(|m| m == "lifetime bound not satisfied"
            || m == "type does not fulfill the required lifetime"),
        "{messages:?}"
    );
    let (id, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "f")
        .unwrap();
    let predicate = &sigs.fn_predicates[id][0];
    let table = &sigs.regions.fns[id].bound_traits[0];
    assert_eq!(&*table.lifetimes, &[Region::Param(0), Region::Param(0)]);
    assert_eq!(&*table.args[0], &[Region::Param(1), Region::Static]);
    let requirements = trait_requirements(&hir.items, &sigs, predicate, &[], table).unwrap();
    assert!(requirements.contains(&Outlives::Region(Region::Param(1), Region::Param(0))));
    assert!(requirements.contains(&Outlives::Region(Region::Static, Region::Param(0))));
    assert!(requirements.contains(&Outlives::Type(
        flux_ty::Ty::param(0, flux_intern::Symbol::intern("T")),
        Region::Param(0)
    )));
    let mut malformed = table.clone();
    malformed.args = Box::default();
    assert!(trait_requirements(&hir.items, &sigs, predicate, &[], &malformed).is_none());
}

#[test]
fn undeclared_nested_names_and_untracked_projections_are_rejected() {
    let source = "trait Tr<T> {} fn f<N: Tr<&'missing i32>>() {} fn main() {}";
    oracle(source, false);
    assert!(
        resolve(source)
            .2
            .iter()
            .any(|m| m.contains("undeclared lifetime"))
    );
    for source in [
        "trait Tr<'a> {} trait Sub<'a>: Tr<'a> {} fn main() {}",
        "trait Tr<'a> {} fn f<'a>(_: &'a dyn Tr<'a>) {} fn main() {}",
        "trait Tr<'a> {} trait Sub<'a> where Self: Tr<'a> {} fn main() {}",
        "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn f() -> impl Tr<'static> { S } fn main() {}",
        "trait Tr<'a> { type Out; } fn f<'a, T: Tr<'a>>(_: <T as Tr<'a>>::Out) {} fn main() {}",
    ] {
        oracle(source, true);
        let messages = resolve(source).2;
        assert!(
            messages
                .iter()
                .any(|m| m.contains("supertraits and associated type bounds")
                    || m.contains("lifetime arguments on trait objects")
                    || m.contains("opaque trait arguments")
                    || m.contains("qualified trait projections")),
            "{source}: {messages:?}"
        );
    }
}

#[test]
fn abstract_self_outlives_is_elaborated() {
    let source = "trait Tr<'a> where Self: 'a {} fn f<'a, N: Tr<'a>>() {} fn main() {}";
    oracle(source, true);
    let (hir, sigs, messages) = resolve(source);
    assert_eq!(
        messages,
        ["lifetime parameters on traits are not supported yet"]
    );
    let (id, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "f")
        .unwrap();
    assert!(sigs.regions.fns[id].outlives.contains(&Outlives::Type(
        flux_ty::Ty::param(0, flux_intern::Symbol::intern("N")),
        Region::Param(0)
    )));
}

#[test]
fn omitted_region_bearing_defaults_are_explicitly_rejected() {
    for source in [
        "trait Tr<T = &'static i32> {} fn f<N: Tr>() {} fn main() {}",
        "trait Tr<T = &'static i32> {} struct S; impl Tr for S {} fn main() {}",
        "trait Tr<T = &'static i32> {} impl<'a> Tr for &'a i32 {} fn main() {}",
    ] {
        oracle(source, true);
        assert!(
            resolve(source)
                .2
                .iter()
                .any(|m| m.contains("default trait type arguments"))
        );
    }
}

#[test]
fn impl_header_uses_outlives_elaborated_from_its_bounds() {
    let source = "trait Bound<'a> where Self: 'a {} trait Tr<'a, T: 'a> {} struct S<T>(T); impl<'a, T: Bound<'a>> Tr<'a, T> for S<T> {} fn main() {}";
    oracle(source, true);
    let messages = resolve(source).2;
    assert!(
        messages
            .iter()
            .all(|m| m == "lifetime parameters on traits are not supported yet"),
        "{messages:?}"
    );
}

#[test]
fn nested_array_const_arguments_have_no_outlives_requirement() {
    let source = "trait Tr<'a, T: 'a> {} fn f<'a, T: 'a, const K: usize, N: Tr<'a, &'a [T; K]>>() {} fn main() {}";
    oracle(source, true);
    let messages = resolve(source).2;
    assert!(
        !messages
            .iter()
            .any(|m| m == "type does not fulfill the required lifetime"
                || m == "lifetime bound not satisfied"),
        "{messages:?}"
    );
}
