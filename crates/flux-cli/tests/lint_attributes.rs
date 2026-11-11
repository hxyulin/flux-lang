//! Native diagnostics and hard-error boundaries for overflowing-literal levels.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::{path::Path, process::Command, time::Duration};
#[test]
fn lint_warnings_do_not_fail_builds_or_hide_const_errors() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/lint-cli"),
    );
    let root = scratch.path();
    for (name, text, accepted, marker) in [
        (
            "warn",
            "#[warn(overflowing_literals)] fn main(){let _x:i8=255;}",
            true,
            "warning: literal out of range",
        ),
        (
            "expect",
            "#[expect(overflowing_literals)] fn main(){let _x:i8=255;}",
            true,
            "",
        ),
        (
            "unfulfilled",
            "#[expect(overflowing_literals)] fn main(){}",
            true,
            "warning: this lint expectation is unfulfilled",
        ),
        (
            "deny",
            "#[deny(overflowing_literals)] fn main(){let _x:i8=255;}",
            false,
            "error: literal out of range",
        ),
        (
            "forbid",
            "#![forbid(overflowing_literals)] #[allow(overflowing_literals)] fn main(){}",
            false,
            "incompatible with previous forbid",
        ),
        (
            "constant",
            "#![allow(overflowing_literals)] const BAD:u8=255+1;fn main(){let _=BAD;}",
            false,
            "overflow",
        ),
    ] {
        let source = root.join(format!("{name}.rs"));
        std::fs::write(&source, text).unwrap();
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("build")
                .arg(&source)
                .arg("--fast-build")
                .arg("-o")
                .arg(root.join(name)),
            root,
            name,
            Duration::from_secs(60),
        )
        .completed();
        assert_eq!(
            output.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        if marker.is_empty() {
            assert!(!stderr.contains("warning:"), "{stderr}");
        } else {
            assert!(stderr.contains(marker), "{name}: {stderr}");
        }
    }
}
