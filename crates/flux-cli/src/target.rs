//! Configuration of the native targets admitted by Flux.
use std::collections::BTreeSet;

pub(crate) fn host() -> Result<&'static str, String> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => Ok("aarch64-apple-darwin"),
        ("x86_64", "linux") => Ok("x86_64-unknown-linux-gnu"),
        _ => Err("unsupported native target".into()),
    }
}

pub(crate) fn cfg() -> Result<BTreeSet<String>, String> {
    host()?;
    let macos = std::env::consts::OS == "macos";
    let mut result = BTreeSet::from(["debug_assertions".into(), "unix".into()]);
    for (name, value) in [
        ("target_os", std::env::consts::OS),
        ("target_arch", std::env::consts::ARCH),
        ("target_family", "unix"),
        ("target_vendor", if macos { "apple" } else { "unknown" }),
        ("target_env", if macos { "" } else { "gnu" }),
        ("target_abi", ""),
        ("target_endian", "little"),
        ("target_pointer_width", "64"),
        ("panic", "abort"),
    ] {
        result.insert(format!("{name}={value:?}"));
    }
    for width in ["8", "16", "32", "64", "ptr"] {
        result.insert(format!("target_has_atomic={width:?}"));
    }
    for feature in if macos {
        &["neon"][..]
    } else {
        &["fxsr", "sse", "sse2"][..]
    } {
        result.insert(format!("target_feature={feature:?}"));
    }
    Ok(result)
}
