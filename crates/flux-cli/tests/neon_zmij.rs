//! Exact integer NEON APIs used by the unchanged zmij AArch64 path.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::{process::Command, time::Duration};

#[test]
fn zmij_integer_neon_matches_rustc() {
    support::oracle::compare_fixture("neon_zmij");
}

#[test]
fn immediate_boundaries_match_rustc() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/neon-zmij-immediates/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, expected) in [
        (
            "legacy_left",
            "let _=vshll_n_u16(core::mem::transmute([1u16;4]),16);",
            true,
        ),
        (
            "legacy_right",
            "let _=vshr_n_u32(core::mem::transmute([1u32;2]),32);",
            true,
        ),
        (
            "legacy_lane",
            "let _=vsetq_lane_u64(7,core::mem::transmute([1u64;2]),1);",
            true,
        ),
        (
            "legacy_bad_lane",
            "let _=vsetq_lane_u64(7,core::mem::transmute([1u64;2]),2);",
            false,
        ),
        (
            "left_zero",
            "let _=vshll_n_u16::<0>(core::mem::transmute([1u16;4]));",
            true,
        ),
        (
            "left_max",
            "let _=vshll_n_u16::<16>(core::mem::transmute([1u16;4]));",
            true,
        ),
        (
            "left_negative",
            "let _=vshll_n_u16::<-1>(core::mem::transmute([1u16;4]));",
            false,
        ),
        (
            "left_excess",
            "let _=vshll_n_u16::<17>(core::mem::transmute([1u16;4]));",
            false,
        ),
        (
            "right_one",
            "let _=vshr_n_u32::<1>(core::mem::transmute([1u32;2]));",
            true,
        ),
        (
            "right_max",
            "let _=vshr_n_u32::<32>(core::mem::transmute([1u32;2]));",
            true,
        ),
        (
            "right_zero",
            "let _=vshr_n_u32::<0>(core::mem::transmute([1u32;2]));",
            false,
        ),
        (
            "right_excess",
            "let _=vshr_n_u32::<33>(core::mem::transmute([1u32;2]));",
            false,
        ),
        (
            "right_negative",
            "let _=vshr_n_u32::<-1>(core::mem::transmute([1u32;2]));",
            false,
        ),
        (
            "lane_zero",
            "let _=vsetq_lane_u64::<0>(7,core::mem::transmute([1u64;2]));",
            true,
        ),
        (
            "lane_one",
            "let _=vsetq_lane_u64::<1>(7,core::mem::transmute([1u64;2]));",
            true,
        ),
        (
            "lane_excess",
            "let _=vsetq_lane_u64::<2>(7,core::mem::transmute([1u64;2]));",
            false,
        ),
        (
            "lane_negative",
            "let _=vsetq_lane_u64::<-1>(7,core::mem::transmute([1u64;2]));",
            false,
        ),
        (
            "reified_invalid_shift",
            "let f:unsafe fn(uint16x4_t)->uint32x4_t=vshll_n_u16::<17>;let _=f(core::mem::transmute([1u16;4]));",
            false,
        ),
        (
            "reified_invalid_lane",
            "let f:unsafe fn(u64,uint64x2_t)->uint64x2_t=vsetq_lane_u64::<2>;let _=f(7,core::mem::transmute([1u64;2]));",
            false,
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("use core::arch::aarch64::*;fn main(){{unsafe{{{body}}}}}"),
        )
        .unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_flux")] {
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
                expected,
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
