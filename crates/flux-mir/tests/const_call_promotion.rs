//! Const initializers retain immutable call results, without widening runtime promotion.
#[test]
fn constant_call_temporaries_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-const-call-promotion-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepted) in [
        (
            "constant_call_array",
            "const fn value()->u32{37}const fn hold(v:&'static[u32])->&'static[u32]{v}const C:&[u32]=hold(&[value()]);",
            true,
        ),
        (
            "static_const_method_array",
            "struct Name(&'static str);impl Name{const fn text(&self)->&str{self.0}}const N:Name=Name(\"field\");const fn hold(v:&'static[&'static str])->&'static[&'static str]{v}static C:&[&str]=hold(&[N.text()]);",
            true,
        ),
        (
            "runtime_call_not_promoted",
            "const fn value()->u32{37}fn bad()->&'static[u32]{&[value()]}",
            false,
        ),
        (
            "const_function_call_not_promoted",
            "const fn value()->u32{37}const fn bad()->&'static[u32]{&[value()]}",
            false,
        ),
        (
            "local_not_extended",
            "const C:&u32={let value=37;&value};",
            false,
        ),
        (
            "mutable_not_extended",
            "const fn hold(v:&'static mut[u32])->&'static mut[u32]{v}const C:&mut[u32]=hold(&mut[37]);",
            false,
        ),
        (
            "interior_not_extended",
            "use std::cell::Cell;const fn hold(v:&'static Cell<u32>)->&'static Cell<u32>{v}const C:&Cell<u32>=hold(&Cell::new(37));",
            false,
        ),
        (
            "panic_still_evaluated",
            "const fn bad()->u32{panic!(\"constant failure\")}const fn hold(v:&'static[u32])->&'static[u32]{v}const C:&[u32]=hold(&[bad()]);",
            false,
        ),
    ] {
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        let typed = flux_mir::check_file(&hir);
        let mut diagnostics = hir.diagnostics.clone();
        diagnostics.extend(typed.diagnostics.iter().cloned());
        if diagnostics.is_empty() {
            diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}
