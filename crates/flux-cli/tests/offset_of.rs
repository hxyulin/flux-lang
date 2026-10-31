//! Constant and generic field offsets use the same layout as code generation.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn field_offsets_match_rustc() {
    support::oracle::compare_fixture("offset_of");
}

#[test]
fn invalid_field_offsets_match_rustc() {
    use std::process::Command;
    use std::time::Duration;
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-offset-errors"));
    for (name, source) in [
        (
            "unconstrained",
            "struct Pair<T> { tag:u8, value:T } fn main() { let _=core::mem::offset_of!(Pair<_>, tag); }",
        ),
        (
            "missing",
            "struct S { x:u32 } fn main() { let _=core::mem::offset_of!(S, y); }",
        ),
        (
            "private",
            "mod m { pub struct S { x:u32 } } fn main() { let _=core::mem::offset_of!(m::S, x); }",
        ),
        (
            "deref",
            "struct S { x: &'static (u32,u32) } fn main() { let _=core::mem::offset_of!(S, x.0); }",
        ),
        (
            "tuple",
            "fn main() { let _=core::mem::offset_of!((u32,u32), 2); }",
        ),
        (
            "malformed",
            "struct S { x:u32 } fn main() { let _=core::mem::offset_of!(S, x..x); }",
        ),
    ] {
        let path = scratch.path().join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(scratch.path().join("probe.rmeta")),
            scratch.path(),
            &format!("rustc-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        let actual = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("mir")
                .arg(&path),
            scratch.path(),
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert!(!oracle.status.success(), "invalid rustc probe {name}");
        assert!(!actual.status.success(), "invalid Flux probe {name}");
    }
}
