//! Foreign data declarations and literal native dynamic-library requirements.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn foreign_statics_and_dynamic_libraries_match_rustc() {
    support::oracle::compare_fixture("foreign_statics");
}

#[test]
fn foreign_data_and_link_contracts_match_rustc() {
    use std::{fs, process::Command};
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/foreign-data-contracts/{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).unwrap();
    for (name, text, rust_accepts, flux_accepts) in [
        (
            "read",
            "unsafe extern \"C\"{static X:u8;} fn f()->u8{unsafe{X}} fn main(){}",
            true,
            true,
        ),
        (
            "unsafe_read",
            "unsafe extern \"C\"{static X:u8;} fn f()->u8{X} fn main(){}",
            false,
            false,
        ),
        (
            "unsafe_ref",
            "unsafe extern \"C\"{static X:u8;} fn f()->&'static u8{&X} fn main(){}",
            false,
            false,
        ),
        (
            "raw",
            "unsafe extern \"C\"{static X:u8;static mut Y:u8;}const P:*const u8=&raw const X; fn q()->*mut u8{&raw mut Y} fn main(){}",
            true,
            true,
        ),
        (
            "immutable",
            "unsafe extern \"C\"{static X:u8;} fn f(){unsafe{X=1;}} fn main(){}",
            false,
            false,
        ),
        (
            "foreign_nonsync",
            "unsafe extern \"C\"{static X:*mut u8;} fn main(){}",
            true,
            true,
        ),
        (
            "initializer",
            "unsafe extern \"C\"{static X:u8=1;} fn main(){}",
            false,
            false,
        ),
        (
            "missing_type",
            "unsafe extern \"C\"{static X;} fn main(){}",
            false,
            false,
        ),
        (
            "link",
            "#[link(name=\"c\",kind=\"dylib\",)]unsafe extern \"C\"{} fn main(){}",
            true,
            true,
        ),
        (
            "link_raw",
            "#[link(name=r#\"c\"#)]unsafe extern \"C\"{} fn main(){}",
            true,
            true,
        ),
        (
            "link_cfg",
            "#[cfg_attr(unix,link(name=\"c\"))]unsafe extern \"C\"{} fn main(){}",
            true,
            true,
        ),
        (
            "link_missing",
            "#[link(kind=\"dylib\")]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        (
            "link_duplicate",
            "#[link(name=\"c\",name=\"m\")]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        (
            "link_kind",
            "#[link(name=\"c\",kind=\"other\")]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        (
            "link_empty",
            "#[link(name=\"\")]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        (
            "link_nul",
            "#[link(name=\"c\\0\")]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        (
            "link_integer",
            "#[link(name=1)]unsafe extern \"C\"{} fn main(){}",
            false,
            false,
        ),
        // These stable Rust features require distinct archive/framework and
        // safe-extern-static semantics, rather than discarded attributes.
        (
            "static_library",
            "#[link(name=\"c\",kind=\"static\")]unsafe extern \"C\"{} fn main(){}",
            true,
            false,
        ),
        (
            "framework",
            "#[link(name=\"CoreFoundation\",kind=\"framework\")]unsafe extern \"C\"{} fn main(){}",
            true,
            false,
        ),
        (
            "safe_static",
            "unsafe extern \"C\"{safe static X:u8;} fn f()->u8{X} fn main(){}",
            true,
            false,
        ),
    ] {
        let path = directory.join(format!("{name}.rs"));
        fs::write(&path, text).unwrap();
        let rust = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(directory.join(format!("{name}.rmeta"))),
            &directory,
            &format!("rust-{name}"),
            std::time::Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            rust.status.success(),
            rust_accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let flux = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("mir")
                .arg(&path),
            &directory,
            &format!("flux-{name}"),
            std::time::Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            flux.status.success(),
            flux_accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
}
