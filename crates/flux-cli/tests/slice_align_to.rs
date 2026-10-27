//! Shared aligned slice views preserve bounds, storage and input lifetimes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::process::Command;
use std::time::Duration;

#[test]
fn slice_align_to_matches_rustc() {
    support::oracle::compare_fixture("slice_align_to");
}

#[test]
fn slice_align_to_boundaries_match_rustc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/slice-align-to/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "unsafe_call",
            "fn main(){let bytes=[0u8;16];let _=bytes.align_to::<u64>();}",
            false,
        ),
        (
            "escaped",
            "fn f()->&'static[u64]{let bytes=[0u8;16];unsafe{bytes.align_to::<u64>().1}}fn main(){}",
            false,
        ),
        (
            "shared",
            "fn f(bytes:&[u8])->&[u64]{unsafe{bytes.align_to::<u64>().1}}fn main(){}",
            true,
        ),
        (
            "mutate_borrowed",
            "fn main(){let mut bytes=[0u8;16];let middle=unsafe{bytes.align_to::<u64>().1};bytes[0]=1;println!(\"{}\",middle.len());}",
            false,
        ),
        (
            "unsized_output",
            "fn main(){let bytes=[0u8;16];let _=unsafe{bytes.align_to::<[u8]>()};}",
            false,
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
