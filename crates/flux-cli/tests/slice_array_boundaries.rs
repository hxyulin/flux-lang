//! Compile-only rejection checks: invalid Rust is never executed.
use std::path::Path;
use std::process::Command;

#[test]
fn slice_array_rejections_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir =
        std::env::temp_dir().join(format!("flux-slice-array-negative-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [(
        "non_copy",
        "fn main() { let a = [String::from(\"a\")]; let s: &[String] = &a; let _: [String; 1] = s.try_into().unwrap(); }",
    )] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert!(!rustc.status.success(), "rustc accepted {name}");
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(&root)
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert!(!flux.status.success(), "flux accepted {name}");
        let diagnostics = String::from_utf8_lossy(&flux.stderr);
        assert!(diagnostics.contains("error:"), "{name}: {diagnostics}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
