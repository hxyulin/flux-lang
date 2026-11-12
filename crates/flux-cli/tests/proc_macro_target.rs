//! The standard target API has no active macro-host context.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::process::Command;
use std::time::Duration;
#[test]
fn empty_target_streams_match_rustc() {
    support::oracle::compare_fixture("proc_macro_target");
}
#[test]
fn host_operations_panic_outside_invocations() {
    let scratch = support::scratch::Scratch::new("out/proc-macro-target-panic");
    let root = scratch.path();
    for (name, expression) in [
        ("parse", r#""".parse::<proc_macro::TokenStream>()"#),
        ("span", "proc_macro::Span::call_site()"),
        ("mixed", "proc_macro::Span::mixed_site()"),
        (
            "punct",
            "proc_macro::Punct::new('+',proc_macro::Spacing::Alone)",
        ),
        ("literal", "proc_macro::Literal::u8_unsuffixed(1)"),
        (
            "group",
            "proc_macro::Group::new(proc_macro::Delimiter::None,proc_macro::TokenStream::new())",
        ),
    ] {
        let source = root.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("extern crate proc_macro;fn main(){{let _={expression};}}"),
        )
        .unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_fluxc")] {
            let executable = root.join(if compiler == "rustc" { "rustc" } else { "flux" });
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.args(["--edition=2024", "-C", "panic=abort"]);
            } else {
                command.arg("build");
            }
            let build = support::process::run(
                command.arg(&source).arg("-o").arg(&executable),
                root,
                "build",
                Duration::from_secs(60),
            )
            .completed();
            support::oracle::success(&build, name);
            let output = support::process::run(
                &mut Command::new(&executable),
                root,
                "run",
                Duration::from_secs(30),
            )
            .completed();
            assert!(!output.status.success(), "{name} {compiler}");
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("procedural macro API is used outside of a procedural macro"),
                "{name} {compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
