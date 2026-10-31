//! The std facade exposes the vector pipeline used by Keel's scanner.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::process::Command;
use std::time::Duration;

#[test]
fn neon_scanning_matches_rustc() {
    support::oracle::compare_fixture("neon_scanning");
}

#[test]
fn std_neon_boundaries_match_rustc() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/neon-scanning/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, accepted) in [
        (
            "valid_shift_one",
            "unsafe { let _ = vshrn_n_u16::<1>(vreinterpretq_u16_u8(vdupq_n_u8(3))); }",
            true,
        ),
        (
            "valid_shift_eight",
            "unsafe { let _ = vshrn_n_u16::<8>(vreinterpretq_u16_u8(vdupq_n_u8(3))); }",
            true,
        ),
        (
            "zero_shift",
            "unsafe { let _ = vshrn_n_u16::<0>(vreinterpretq_u16_u8(vdupq_n_u8(3))); }",
            false,
        ),
        (
            "excess_shift",
            "unsafe { let _ = vshrn_n_u16::<9>(vreinterpretq_u16_u8(vdupq_n_u8(3))); }",
            false,
        ),
        (
            "negative_shift",
            "unsafe { let _ = vshrn_n_u16::<-1>(vreinterpretq_u16_u8(vdupq_n_u8(3))); }",
            false,
        ),
        (
            "invalid_lane",
            "unsafe { let _ = vget_lane_u64::<1>(vreinterpret_u64_u8(vdup_n_u8(3))); }",
            false,
        ),
        (
            "unsafe_load",
            "let bytes = [0u8; 16]; let _ = vld1q_u8(bytes.as_ptr());",
            false,
        ),
        ("unsafe_splat", "let _ = vdupq_n_u8(3);", false),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("use std::arch::aarch64::*; fn main() {{ {body} }}"),
        )
        .unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_fluxc")] {
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.arg("--edition=2024");
            } else {
                command.arg("build");
            }
            let result = support::process::run(
                command
                    .arg(&source)
                    .arg("-o")
                    .arg(directory.join("program")),
                &directory,
                &format!(
                    "{name}-{}",
                    if compiler == "rustc" { "rustc" } else { "flux" }
                ),
                Duration::from_secs(60),
            )
            .completed();
            assert_eq!(
                result.status.success(),
                accepted,
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
