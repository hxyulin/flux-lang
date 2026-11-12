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
            "half_left_min",
            "let _=vshlq_n_u16::<0>(core::mem::transmute([1u16;8]));",
            true,
        ),
        (
            "half_left_max",
            "let _=vshlq_n_u16::<15>(core::mem::transmute([1u16;8]));",
            true,
        ),
        (
            "half_left_invalid",
            "let f:unsafe fn(uint16x8_t)->uint16x8_t=vshlq_n_u16::<16>;let _=f(core::mem::transmute([1u16;8]));",
            false,
        ),
        (
            "half_right_full",
            "let _=vshrq_n_u16::<16>(core::mem::transmute([1u16;8]));",
            true,
        ),
        (
            "half_right_zero",
            "let _=vshrq_n_u16::<0>(core::mem::transmute([1u16;8]));",
            false,
        ),
        (
            "half_right_invalid",
            "let _=vshrq_n_u16::<17>(core::mem::transmute([1u16;8]));",
            false,
        ),
        (
            "word_lane_last",
            "let _=vgetq_lane_u32::<3>(core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "word_lane_invalid",
            "let f:unsafe fn(uint32x4_t)->u32=vgetq_lane_u32::<4>;let _=f(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshlq_n_u32_min",
            "let _=vshlq_n_u32::<0>(core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vshlq_n_u32_max",
            "let _=vshlq_n_u32::<31>(core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vshlq_n_u32_below",
            "let _=vshlq_n_u32::<-1>(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshlq_n_u32_above",
            "let _=vshlq_n_u32::<32>(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshlq_n_u32_reified_invalid",
            "let f:unsafe fn(uint32x4_t)->uint32x4_t=vshlq_n_u32::<32>;let _=f(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshrq_n_u32_min",
            "let _=vshrq_n_u32::<1>(core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vshrq_n_u32_max",
            "let _=vshrq_n_u32::<32>(core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vshrq_n_u32_below",
            "let _=vshrq_n_u32::<0>(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshrq_n_u32_above",
            "let _=vshrq_n_u32::<33>(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshrq_n_u32_reified_invalid",
            "let f:unsafe fn(uint32x4_t)->uint32x4_t=vshrq_n_u32::<33>;let _=f(core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vshrq_n_u8_min",
            "let _=vshrq_n_u8::<1>(core::mem::transmute([1u8;16]));",
            true,
        ),
        (
            "vshrq_n_u8_max",
            "let _=vshrq_n_u8::<8>(core::mem::transmute([1u8;16]));",
            true,
        ),
        (
            "vshrq_n_u8_below",
            "let _=vshrq_n_u8::<0>(core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vshrq_n_u8_above",
            "let _=vshrq_n_u8::<9>(core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vshrq_n_u8_reified_invalid",
            "let f:unsafe fn(uint8x16_t)->uint8x16_t=vshrq_n_u8::<9>;let _=f(core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vextq_u8_min",
            "let _=vextq_u8::<0>(core::mem::transmute([1u8;16]), core::mem::transmute([1u8;16]));",
            true,
        ),
        (
            "vextq_u8_max",
            "let _=vextq_u8::<15>(core::mem::transmute([1u8;16]), core::mem::transmute([1u8;16]));",
            true,
        ),
        (
            "vextq_u8_below",
            "let _=vextq_u8::<-1>(core::mem::transmute([1u8;16]), core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vextq_u8_above",
            "let _=vextq_u8::<16>(core::mem::transmute([1u8;16]), core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vextq_u8_reified_invalid",
            "let f:unsafe fn(uint8x16_t,uint8x16_t)->uint8x16_t=vextq_u8::<16>;let _=f(core::mem::transmute([1u8;16]), core::mem::transmute([1u8;16]));",
            false,
        ),
        (
            "vextq_u32_min",
            "let _=vextq_u32::<0>(core::mem::transmute([1u32;4]), core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vextq_u32_max",
            "let _=vextq_u32::<3>(core::mem::transmute([1u32;4]), core::mem::transmute([1u32;4]));",
            true,
        ),
        (
            "vextq_u32_below",
            "let _=vextq_u32::<-1>(core::mem::transmute([1u32;4]), core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vextq_u32_above",
            "let _=vextq_u32::<4>(core::mem::transmute([1u32;4]), core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "vextq_u32_reified_invalid",
            "let f:unsafe fn(uint32x4_t,uint32x4_t)->uint32x4_t=vextq_u32::<4>;let _=f(core::mem::transmute([1u32;4]), core::mem::transmute([1u32;4]));",
            false,
        ),
        (
            "legacy_left",
            "let _=vshll_n_u16(core::mem::transmute([1u16;4]),16);",
            true,
        ),
        (
            "legacy_arithmetic",
            "let _=vshll_n_u16(core::mem::transmute([1u16;4]),8+8);",
            true,
        ),
        (
            "legacy_const_call",
            "const fn amount()->i32{8} let _=vshll_n_u16(core::mem::transmute([1u16;4]),amount()+1);",
            true,
        ),
        (
            "legacy_macro_arithmetic",
            "macro_rules! shift { () => { vshll_n_u16(core::mem::transmute([1u16;4]),16-9) }; } let _=shift!();",
            true,
        ),
        (
            "legacy_generic_arithmetic",
            "fn f<const N:i32>() { unsafe { let _=vshll_n_u16(core::mem::transmute([1u16;4]),N+1); } }",
            false,
        ),
        (
            "legacy_shadow",
            "let vshll_n_u16=|_:uint16x4_t,n:i32|n;let n=7;let _=vshll_n_u16(core::mem::transmute([1u16;4]),n+1);",
            true,
        ),
        (
            "legacy_runtime",
            "let n=8;let _=vshll_n_u16(core::mem::transmute([1u16;4]),n+1);",
            false,
        ),
        (
            "legacy_bad_arithmetic",
            "let _=vshll_n_u16(core::mem::transmute([1u16;4]),16+1);",
            false,
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
                expected,
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}
