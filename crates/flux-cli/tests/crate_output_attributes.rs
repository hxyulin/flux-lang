//! Explicit driver output overrides valid source output requests.
mod flib_support;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;
use std::process::Command;

#[test]
fn explicit_output_matches_rustc_attribute_precedence() {
    let s = Scratch::new(std::env::temp_dir().join("flux-crate-output"));
    for (name, attribute, accepted) in [
        (
            "mixed",
            "#![crate_type=\"rlib\"]\n#![crate_type=\"dylib\"]",
            true,
        ),
        ("staticlib", "#![crate_type=\"staticlib\"]", true),
        ("cdylib", "#![crate_type=\"cdylib\"]", true),
        ("proc_macro", "#![crate_type=\"proc-macro\"]", true),
        ("unknown", "#![crate_type=\"unknown\"]", false),
        ("number", "#![crate_type=42]", false),
        ("list", "#![crate_type(\"rlib\")]", false),
        ("unsafe_attr", "#![unsafe(crate_type=\"rlib\")]", false),
    ] {
        let source = s.path().join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("{attribute}\npub fn value() -> u8 {{ 42 }}\nfn main() {{}}"),
        )
        .unwrap();
        for binary in [false, true] {
            let oracle = flib_support::run(
                Command::new("rustc")
                    .args([
                        "--edition=2024",
                        "--emit=metadata",
                        "--crate-type",
                        if binary { "bin" } else { "rlib" },
                    ])
                    .arg(&source)
                    .arg("-o")
                    .arg(s.path().join("oracle.rmeta")),
                "crate-output-rustc",
            );
            assert_eq!(
                oracle.status.success(),
                accepted,
                "rustc {name}: {}",
                String::from_utf8_lossy(&oracle.stderr)
            );
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command
                .arg("build")
                .arg(&source)
                .args(["--crate-type", if binary { "bin" } else { "lib" }]);
            if !binary {
                command.arg("--metadata-only");
            }
            command.arg("-o").arg(
                s.path()
                    .join(if binary { "program" } else { "output.flib" }),
            );
            let output = flib_support::run(&mut command, "crate-output-flux");
            assert_eq!(
                output.status.success(),
                accepted,
                "Flux {name}, binary={binary}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
