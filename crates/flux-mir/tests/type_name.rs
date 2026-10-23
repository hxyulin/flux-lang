//! Stable type_name remains non-const and does not expose intrinsic internals.
use std::process::Command;

#[test]
fn type_name_api_boundaries_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-type-name-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [
        ("unconstrained", "fn main(){let _=core::any::type_name();}"),
        (
            "constant",
            "const NAME:&str=std::any::type_name::<u8>(); fn main(){let _=NAME;}",
        ),
        (
            "const_function",
            "const fn name()->&'static str{core::any::type_name::<u8>()} fn main(){}",
        ),
        (
            "private",
            "fn main(){let _=core::intrinsics::type_name::<u8>();}",
        ),
    ] {
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!rustc.status.success(), "rustc {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let typed = flux_mir::check_file(&hir);
        let errors: Vec<_> = hir.diagnostics.iter().chain(&typed.diagnostics).collect();
        assert!(!errors.is_empty(), "flux {name}");
        assert!(
            errors.iter().any(|d| d.message.contains("non-const")
                || d.message.contains("private")
                || d.message.contains("infer")
                || d.message.contains("annotations")),
            "{name}: {errors:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
