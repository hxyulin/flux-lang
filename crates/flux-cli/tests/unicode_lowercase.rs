//! Unicode default casing and original-string sigma contexts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
mod string {
    pub use std::string::String;
}
#[path = "../../../library/alloc/src/str/lowercase.rs"]
mod actual;

#[test]
fn lowercase_all_scalars_and_contexts_match_rustc() {
    assert_eq!(std::char::UNICODE_VERSION, (17, 0, 0));
    for code in 0..=0x10ffffu32 {
        let Some(value) = char::from_u32(code) else {
            continue;
        };
        for text in [
            value.to_string(),
            format!("{value}Σ"),
            format!("A{value}Σ"),
            format!("AΣ{value}"),
            format!("AΣ{value}A"),
        ] {
            assert_eq!(
                actual::convert(&text),
                text.to_lowercase(),
                "U+{code:04x}: {text:?}"
            );
        }
    }
}

#[test]
fn unicode_lowercase_matches_rustc_off_and_on() {
    use std::{path::Path, process::Command, time::Duration};
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/unicode-lowercase/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let scalars = (0..=0x10ffffu32)
        .filter_map(char::from_u32)
        .filter(|&value| {
            value.is_ascii()
                || actual::is_cased(value)
                || actual::is_case_ignorable(value)
                || value.to_lowercase().ne(std::iter::once(value))
        });
    let mut cases = String::new();
    for value in scalars {
        cases.push(value);
        cases.push('\0');
        cases.push_str(&format!("{value}Σ\0A{value}Σ\0AΣ{value}\0AΣ{value}A\0"));
    }
    std::fs::write(root.join("unicode-cases.txt"), &cases).unwrap();
    let source = root.join("consumer.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/unicode_lowercase/consumer.rs.txt"),
    )
    .unwrap();
    let reference = root.join("reference");
    let built = support::process::run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "rust-build",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&built, "unicode lowercase Rust build");
    let expected = support::process::run(
        &mut Command::new(reference),
        &root,
        "rust-run",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&expected, "unicode lowercase Rust run");
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut build = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        build.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            build.arg("-O");
        }
        let built = support::process::run(&mut build, &root, "flux-build", Duration::from_secs(60))
            .completed();
        support::oracle::success(&built, "unicode lowercase Flux build");
        let output = support::process::run(
            &mut Command::new(executable),
            &root,
            "flux-run",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "unicode lowercase Flux run");
        assert_eq!(output.stdout, expected.stdout);
        assert_eq!(output.stderr, expected.stderr);
    }
}
