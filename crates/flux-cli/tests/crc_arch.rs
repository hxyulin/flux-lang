//! CRC state semantics and runtime feature detection preserve Rust boundaries.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::process::Command;
use std::time::Duration;

#[test]
fn crc_arch_matches_rustc() {
    support::oracle::compare_fixture("crc_arch");
}

#[test]
fn crc_contexts_match_rustc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/crc-arch/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "root_macro",
            "fn main(){let _=std::is_aarch64_feature_detected!(\"crc\");}",
            false,
        ),
        (
            "arch_macro",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(\"crc\");}",
            true,
        ),
        (
            "imported_macro",
            "use std::arch::is_aarch64_feature_detected as has;fn main(){let _=has!(\"crc\");}",
            true,
        ),
        (
            "explicit_std",
            "#![no_std]\nextern crate std;fn main(){let _=std::arch::is_aarch64_feature_detected!(\"crc\");}",
            true,
        ),
        (
            "root_import",
            "use std::is_aarch64_feature_detected as has;fn main(){let _=has!(\"crc\");}",
            false,
        ),
        (
            "invalid_feature",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(\"not-real\");}",
            false,
        ),
        (
            "raw_literal",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(r\"crc\");}",
            false,
        ),
        (
            "concatenated_literal",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(concat!(\"c\",\"rc\"));}",
            false,
        ),
        (
            "trailing_comma",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(\"crc\",);}",
            true,
        ),
        (
            "nonliteral",
            "fn main(){let feature=\"crc\";let _=std::arch::is_aarch64_feature_detected!(feature);}",
            false,
        ),
        (
            "wrong_literal",
            "fn main(){let _=std::arch::is_aarch64_feature_detected!(3);}",
            false,
        ),
        (
            "unsafe_byte",
            "fn main(){let _=core::arch::aarch64::__crc32b(0,0);}",
            false,
        ),
        (
            "unsafe_word",
            "fn main(){let _=core::arch::aarch64::__crc32d(0,0);}",
            false,
        ),
        (
            "safe_pointer",
            "fn main(){let _:fn(u32,u8)->u32=core::arch::aarch64::__crc32b;}",
            false,
        ),
        (
            "feature_call",
            "#[target_feature(enable=\"crc\")]unsafe fn f(){}fn main(){f();}",
            false,
        ),
        (
            "feature_inline",
            "#[inline(always)]#[target_feature(enable=\"crc\")]unsafe fn f(){}fn main(){}",
            false,
        ),
        (
            "unknown_target_feature",
            "#[target_feature(enable=\"not-real\")]unsafe fn f(){}fn main(){}",
            false,
        ),
        (
            "crc_const",
            "const X:u32=unsafe{core::arch::aarch64::__crc32b(0,0)};fn main(){}",
            false,
        ),
        (
            "reified",
            "fn main(){let _:unsafe fn(u32,u64)->u32=core::arch::aarch64::__crc32d;}",
            true,
        ),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_flux")] {
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.arg("--edition=2024");
            } else {
                command.arg("build");
            }
            let output = support::process::run(
                command.arg(&path).arg("-o").arg(root.join("program")),
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

#[test]
fn safe_target_feature_obeys_call_metadata() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/crc-safe-feature/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("safe.rs");
    std::fs::write(
        &source,
        "#[target_feature(enable=\"crc\")]fn f(){}fn main(){unsafe{f();}}",
    )
    .unwrap();
    let reference = support::process::run(
        Command::new("rustc")
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(root.join("rustc")),
        &root,
        "rustc",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&reference, "Rust supports safe target-feature functions");
    let output = support::process::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(root.join("flux")),
        &root,
        "flux",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&output, "Flux checks safe target-feature calls");
}
