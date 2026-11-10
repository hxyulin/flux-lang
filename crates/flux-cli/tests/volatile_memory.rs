//! Native volatile scalars and fail-closed unsupported layouts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn volatile_memory_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("volatile_memory");
}
#[test]
fn unsupported_aggregate_volatile_layouts_fail_explicitly() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/volatile-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, text) in [
        (
            "large_array",
            "fn main(){let mut a=[1u8;64];unsafe{std::ptr::write_volatile(&mut a,[2u8;64]);}}",
        ),
        (
            "overaligned",
            "#[repr(align(64))]struct Aligned(u32);fn main(){let a=Aligned(1);let _=unsafe{std::ptr::read_volatile(&a)};}",
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
        support::oracle::success(&output, name);
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("build")
                .arg(&source)
                .arg("-o")
                .arg(directory.join(name)),
            &directory,
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert!(!output.status.success(), "accepted {name}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("volatile operations support only scalars"),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
