//! Candidate discovery shares type evidence but never proves lifetime obligations.
use std::process::Command;

#[test]
fn contextual_candidates_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-rigid-solver-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "unsized_source",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>+?Sized>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>+?Sized>Child for T{}",
            true,
        ),
        (
            "unsized_output",
            "trait Source{type Out:?Sized;}trait Tag{type Out;}trait Base{}impl<R:?Sized+Tag<Out=u32>,T:Source<Out=R>>Base for T{}trait Child:Base{}impl<T:Source<Out=U>,U:?Sized+Tag<Out=u32>>Child for T{}",
            true,
        ),
        (
            "scalar_output",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>>Child for T{fn get()->u32{73}}",
            true,
        ),
        (
            "chained_output",
            "trait Source{type Out;}trait Base{type Out;}impl<T:Source<Out=U>,U:Source<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=U>,U:Source<Out=u32>>Child for T{fn get()->u32{73}}",
            true,
        ),
        (
            "borrowed_output",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<Out=&'a u32>>Child for T{}",
            true,
        ),
        (
            "output_outlives_missing",
            "trait Source{type Out;}trait Base{type Out;}impl<R:'static,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<Out=&'a u32>>Child for T{}",
            false,
        ),
        (
            "output_outlives_known",
            "trait Source{type Out;}trait Base{type Out;}impl<R:'static,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a:'static,T:Source<Out=&'a u32>>Child for T{}",
            true,
        ),
        (
            "different_trait_lifetimes",
            "trait Source<'a>{type Out;}trait Base{type Out;}impl<R,T:Source<'static,Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<'a,T:Source<'a,Out=u32>>Child for T{}",
            false,
        ),
        (
            "missing_requirement",
            "trait Source{type Out;}trait Marker{}trait Base{type Out;}impl<R,T:Source<Out=R>+Marker>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>>Child for T{}",
            false,
        ),
        (
            "different_output",
            "trait Source{type Out;}trait Other{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>+Other<Out=R>>Base for T{type Out=R;}trait Child:Base{}impl<T:Source<Out=u32>+Other<Out=u64>>Child for T{}",
            false,
        ),
    ] {
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if matches!(
            name,
            "output_outlives_missing" | "different_trait_lifetimes"
        ) {
            assert!(
                typed.diagnostics.is_empty(),
                "{name} must reach lifetime checking: {:?}",
                typed.diagnostics
            );
        }
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
