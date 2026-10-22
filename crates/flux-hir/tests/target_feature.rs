//! Rustc acceptance oracle for the deliberately bounded baseline attribute.
use std::process::Command;

#[test]
fn target_feature_acceptance_matches_rustc() {
    if std::env::consts::ARCH != "aarch64" {
        return;
    }
    let accepted = [
        "#[target_feature(enable = \"neon\")] unsafe fn f() {}",
        "#[target_feature(enable = r#\"neon,neon\"#, enable = \"neon\",)] #[target_feature(enable = \"neon\")] unsafe fn f() {}",
        "#[target_feature()] unsafe fn f() {}",
        "#[target_feature(enable = \"neon\")] const unsafe fn f() {}",
        "struct S; impl S { #[target_feature(enable = \"neon\")] unsafe fn f(&self) {} }",
        "trait T { #[target_feature(enable = \"neon\")] unsafe fn f() {} } struct S; impl T for S { #[target_feature(enable = \"neon\")] unsafe fn f() {} }",
        "#[cfg_attr(all(), target_feature(enable = \"neon\"), inline)] unsafe fn f() {}",
        "#[cfg_attr(any(), target_feature(enable = \"aes\"))] unsafe fn f() {}",
        "#[inline(never,)] #[target_feature(enable = \"neon\")] unsafe fn f() {}",
    ];
    let rejected = [
        "#[target_feature] unsafe fn f() {}",
        "#[target_feature = \"neon\"] unsafe fn f() {}",
        "#[target_feature(enable)] unsafe fn f() {}",
        "#[target_feature(enable = 1)] unsafe fn f() {}",
        "#[target_feature(disable = \"neon\")] unsafe fn f() {}",
        "#[target_feature(enable = \"\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon,\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon, neon\")] unsafe fn f() {}",
        "#[target_feature(enable = \"sse2\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon\",,)] unsafe fn f() {}",
        "#[unsafe(target_feature(enable = \"neon\"))] unsafe fn f() {}",
        "#[target_feature(enable = \"neon\")] struct S;",
        "#[target_feature(enable = \"neon\")] mod m {}",
        "trait T { #[target_feature(enable = \"neon\")] unsafe fn f(); }",
        "unsafe extern \"C\" { #[target_feature(enable = \"neon\")] fn f(); }",
        "#[inline(always)] #[target_feature(enable = \"neon\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon\")] #[cfg_attr(all(), inline(always))] unsafe fn f() {}",
        "#[inline()] #[target_feature(enable = \"neon\")] unsafe fn f() {}",
        "trait T { #[target_feature(enable = \"neon\")] fn f() {} }",
        "trait T { fn f(); } struct S; impl T for S { #[target_feature(enable = \"neon\")] fn f() {} }",
    ];
    let dir = std::env::temp_dir().join(format!("flux-target-feature-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (sources, expected) in [(&accepted[..], true), (&rejected[..], false)] {
        for source in sources {
            let path = dir.join("probe.rs");
            std::fs::write(&path, source).unwrap();
            let output = Command::new("rustc")
                .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(dir.join("probe.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                expected,
                "{source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let (db, root, hir) = flux_hir::lower_text(&path, source);
            assert!(db.file(root).parse.errors().is_empty(), "{source}");
            assert_eq!(
                hir.diagnostics.is_empty(),
                expected,
                "{source}\n{:?}",
                hir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsupported_target_feature_contexts_remain_errors() {
    for source in [
        "struct S { #[target_feature(enable = \"neon\")] x: u8 }",
        "#[target_feature(enable = \"neon\")] fn f() {}",
        "#[target_feature(enable = \"aes\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon,aes\")] #[target_feature(enable = \"neon\")] unsafe fn f() {}",
        "#[target_feature(enable = \"neon\")] const fn f() {}",
        "#[cfg_attr(all(), target_feature(enable = \"aes\"))] unsafe fn f() {}",
        "#[target_feature(enable = \"avx2\")] unsafe fn f() {}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("probe.rs", source);
        assert!(!hir.diagnostics.is_empty(), "{source}");
    }
    if std::env::consts::ARCH != "aarch64" {
        let (_, _, hir) = flux_hir::lower_text(
            "probe.rs",
            "#[target_feature(enable = \"neon\")] unsafe fn f() {}",
        );
        assert!(!hir.diagnostics.is_empty());
    }
}
