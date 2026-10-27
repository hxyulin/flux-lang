//! Foreign symbol aliases retain Rust names and ordinary-function behavior.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn native_foreign_aliases_match_rustc() {
    support::oracle::compare_fixture("foreign_link_names");
}
#[test]
fn link_name_attribute_contracts_match_rustc() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/link-name-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, text, accepts) in [
        (
            "foreign",
            "unsafe extern \"C\"{#[link_name=\"strlen\"]fn f(p:*const u8)->usize;}fn main(){}",
            true,
        ),
        (
            "raw",
            "unsafe extern \"C\"{#[link_name=r#\"strlen\"#]fn f(p:*const u8)->usize;}fn main(){}",
            true,
        ),
        (
            "normal",
            "#[link_name=\"different\"]fn f(){}fn main(){f();}",
            true,
        ),
        (
            "struct",
            "#[link_name=\"different\"]struct S;fn main(){}",
            true,
        ),
        (
            "duplicate",
            "unsafe extern \"C\"{#[link_name=\"first\"]#[link_name=\"second\"]fn f();}fn main(){}",
            true,
        ),
        (
            "nul",
            "unsafe extern \"C\"{#[link_name=\"bad\\0name\"]fn f();}fn main(){}",
            false,
        ),
        (
            "empty",
            "unsafe extern \"C\"{#[link_name=\"\"]fn f();}fn main(){}",
            false,
        ),
        (
            "missing",
            "unsafe extern \"C\"{#[link_name]fn f();}fn main(){}",
            false,
        ),
        (
            "list",
            "unsafe extern \"C\"{#[link_name(\"f\")]fn f();}fn main(){}",
            false,
        ),
        (
            "integer",
            "unsafe extern \"C\"{#[link_name=7]fn f();}fn main(){}",
            false,
        ),
        (
            "unsafe",
            "unsafe extern \"C\"{#[unsafe(link_name=\"strlen\")]fn f();}fn main(){}",
            false,
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(&source, text).unwrap();
        let output = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&source)
                .arg("-o")
                .arg(directory.join(format!("{name}.rmeta"))),
            &directory,
            &format!("rustc-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .arg("mir")
                .arg(&source),
            &directory,
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            output.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
