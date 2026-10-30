//! Compare integer SSE/AVX operations with the native Rust implementation.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod support;

#[test]
fn integer_vectors_match_rustc() {
    support::oracle::compare_fixture("x86_intrinsics");
    support::oracle::compare_fixture("function_pointer_raw_casts");
}

#[test]
fn feature_and_immediate_contracts_match_rustc() {
    use std::process::Command;
    use std::time::Duration;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/x86-contracts/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "root_detection",
            "fn main(){let _=std::is_x86_feature_detected!(\"avx2\");}",
            true,
        ),
        (
            "arch_detection",
            "use std::arch::is_x86_feature_detected as has;fn main(){let _=has!(\"sse2\",);}",
            true,
        ),
        (
            "unknown_detection",
            "fn main(){let _=is_x86_feature_detected!(\"not-real\");}",
            false,
        ),
        (
            "unknown_feature",
            "#[target_feature(enable=\"not-real\")]fn f(){}fn main(){}",
            false,
        ),
        (
            "safe_feature_context",
            "use core::arch::x86_64::*;#[target_feature(enable=\"sse2\")]fn f()->__m128i{_mm_add_epi32(_mm_setzero_si128(),_mm_set1_epi8(7))}fn main(){unsafe{let _=f();}}",
            true,
        ),
        (
            "function_to_wide_pointer",
            "fn f(){}fn main(){let _=f as *const [u8];}",
            false,
        ),
        (
            "unsafe_load",
            "fn main(){let a=[0u8;16];let _=core::arch::x86_64::_mm_loadu_si128(a.as_ptr().cast());}",
            false,
        ),
        (
            "unsafe_avx_call",
            "#[target_feature(enable=\"avx2\")]fn f(){}fn main(){f();}",
            false,
        ),
        (
            "implied_features",
            "use core::arch::x86_64::*;#[target_feature(enable=\"avx2\")]fn f()->__m128i{_mm_setzero_si128()}fn main(){unsafe{let _=f();}}",
            true,
        ),
        (
            "shift_negative",
            "use core::arch::x86_64::*;fn main(){unsafe{let _=_mm_slli_epi16::<-1>(_mm_setzero_si128());}}",
            false,
        ),
        (
            "shuffle_too_large",
            "use core::arch::x86_64::*;fn main(){unsafe{let _=_mm_shuffle_epi32::<256>(_mm_setzero_si128());}}",
            false,
        ),
        (
            "extract_too_large",
            "use core::arch::x86_64::*;fn main(){unsafe{let _=_mm256_extracti128_si256::<2>(_mm256_setzero_si256());}}",
            false,
        ),
        (
            "legacy_shift",
            "use core::arch::x86_64::*;fn main(){unsafe{let _=_mm_slli_epi16(_mm_setzero_si128(),7);}}",
            true,
        ),
        (
            "reified_shift",
            "use core::arch::x86_64::*;fn main(){let f:unsafe fn(__m128i)->__m128i=_mm_slli_epi16::<3>;unsafe{let _=f(_mm_setzero_si128());}}",
            true,
        ),
        (
            "reified_invalid",
            "use core::arch::x86_64::*;fn main(){let f:unsafe fn(__m128i)->__m128i=_mm_slli_epi16::<256>;unsafe{let _=f(_mm_setzero_si128());}}",
            false,
        ),
    ] {
        let source_path = root.join(format!("{name}.rs"));
        std::fs::write(&source_path, source).unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_flux")] {
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.arg("--edition=2024");
            } else {
                command.arg("build");
            }
            let output = support::process::run(
                command
                    .arg(&source_path)
                    .arg("-o")
                    .arg(root.join("program")),
                &root,
                &format!(
                    "{name}-{}",
                    if compiler == "rustc" { "rustc" } else { "flux" }
                ),
                Duration::from_secs(60),
            )
            .completed();
            assert_eq!(
                output.status.success(),
                accepted,
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
