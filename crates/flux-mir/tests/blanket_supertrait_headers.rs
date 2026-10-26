//! Blanket supertrait candidates must prove every bound from the original header.
use std::process::Command;

#[test]
fn abstract_blanket_supertraits_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-blanket-supertraits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepts) in [
        (
            "equivalent",
            "trait Equivalent<K:?Sized>{}trait Comparable<K:?Sized>:Equivalent<K>{}impl<Q:?Sized,K:?Sized>Equivalent<K> for Q where Q:Eq,K:std::borrow::Borrow<Q>{}impl<Q:?Sized,K:?Sized>Comparable<K> for Q where Q:Ord,K:std::borrow::Borrow<Q>{}fn main(){}",
            true,
        ),
        (
            "missing_equality",
            "trait Equivalent<K:?Sized>{}trait Comparable<K:?Sized>:Equivalent<K>{}impl<Q:?Sized,K:?Sized>Equivalent<K> for Q where Q:Eq,K:std::borrow::Borrow<Q>{}impl<Q:?Sized,K:?Sized>Comparable<K> for Q where K:std::borrow::Borrow<Q>{}fn main(){}",
            false,
        ),
        (
            "static_bound",
            "trait Parent{}trait Child:Parent{}impl<T:?Sized+'static>Parent for T{}impl<T:?Sized+'static>Child for T{}fn main(){}",
            true,
        ),
        (
            "missing_static",
            "trait Parent{}trait Child:Parent{}impl<T:?Sized+'static>Parent for T{}impl<T:?Sized>Child for T{}fn main(){}",
            false,
        ),
        (
            "reference_static_bound",
            "trait Parent{}trait Child:Parent{}impl<'a:'static>Parent for &'a u32{}impl<'a:'static>Child for &'a u32{}fn main(){}",
            true,
        ),
        (
            "missing_reference_static",
            "trait Parent{}trait Child:Parent{}impl<'a:'static>Parent for &'a u32{}impl<'a>Child for &'a u32{}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            typed.diagnostics.is_empty() && mir.diagnostics.is_empty(),
            accepts,
            "{name}: type={:?}, mir={:?}",
            typed.diagnostics,
            mir.diagnostics
        );
    }
}
