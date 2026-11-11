//! A marker's borrowed supertrait arguments retain their independent regions.
#[test]
fn borrowed_marker_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-marker-supertraits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let base = "use std::borrow::Borrow; trait Value{} impl Value for u8{} type Entry<'a>=(&'a u8,Option<&'a(dyn Value+'a)>); trait Valid<'a>:Borrow<[Entry<'a>]>{} impl<'a,const N:usize>Valid<'a>for[Entry<'a>;N]{}";
    for (name, body, accepted) in [
        (
            "borrowed_result",
            "fn first<'a,'b,T:Valid<'a>>(value:&'b T)->&'b Entry<'a>{&value.borrow()[0]}",
            true,
        ),
        (
            "static_escape",
            "fn escape<'a,T:Valid<'a>>(value:&T)->&'static u8{value.borrow()[0].0}",
            false,
        ),
        (
            "local_escape",
            "fn escape<'a>()->Entry<'a>{let value=7u8;let entries:[Entry<'_>;1]=[(&value,None)];let borrowed:&[Entry<'_>]=entries.borrow();borrowed[0]}",
            false,
        ),
        (
            "impl_owner_mismatch",
            "struct Short<'a>(&'a u8);impl<'a>Borrow<&'a u8>for Short<'a>{fn borrow(&self)->&&'a u8{&self.0}}trait Mark<'a>:Borrow<&'a u8>{}impl<'a>Mark<'static>for Short<'a>{}",
            false,
        ),
    ] {
        let source = format!("{base}{body}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(db.syntax_errors().is_empty());
        let typed = flux_mir::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.iter().cloned());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
        assert!(
            errors.iter().all(|d| !d.message.contains("not supported")),
            "{name}: {errors:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
