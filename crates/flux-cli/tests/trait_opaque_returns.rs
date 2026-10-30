//! Trait opaque outputs retain their bounds through generic and concrete dispatch.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn trait_opaque_returns_match_rustc() {
    support::oracle::compare_fixture("trait_opaque_returns");
}

#[test]
fn trait_opaque_contracts_match_rustc() {
    use std::{process::Command, time::Duration};
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-trait-opaque-contracts"));
    for (name, source, accepted) in [
        (
            "bound_lifetime",
            "trait A<'a>{}impl<'a>A<'a> for &'a u32{}trait P{fn f<'a>(&'a self)->impl A<'a>;}struct S(u32);impl P for S{fn f<'a>(&'a self)->impl A<'a>{&self.0}}fn main(){}",
            true,
        ),
        (
            "bound_static",
            "trait A<'a>{}impl<'a>A<'a> for &'a u32{}trait P{fn f(&self)->impl A<'static>;}struct S(u32);impl P for S{fn f(&self)->impl A<'_>{&self.0}}fn main(){}",
            false,
        ),
        (
            "explicit_outlives",
            "trait P{fn f<'a>(&'a self)->impl Iterator<Item=&'a u32>+'a;}struct S(u32);impl P for S{fn f<'a>(&'a self)->impl Iterator<Item=&'a u32>+'a{core::iter::once(&self.0)}}fn main(){}",
            true,
        ),
        (
            "borrow_escape",
            r#"trait P{fn f<'a>(&'a self)->impl Iterator<Item=&'a u32>;}struct S(u32);impl P for S{fn f<'a>(&'a self)->impl Iterator<Item=&'a u32>{core::iter::once(&self.0)}}fn main(){let result;{let s=S(7);result=s.f();}for value in result{println!("{}",value);}}"#,
            false,
        ),
        (
            "static_promise",
            "trait P{fn f(&self)->impl Iterator<Item=&'static u32>;}struct S(u32);impl P for S{fn f(&self)->impl Iterator<Item=&u32>{core::iter::once(&self.0)}}fn main(){}",
            false,
        ),
        (
            "refined",
            "trait P{fn f()->impl Iterator<Item=u32>;}struct S;impl P for S{fn f()->core::ops::Range<u32>{0..2}}fn main(){}",
            true,
        ),
        (
            "wrong_bound",
            "trait P{fn f()->impl Iterator<Item=u32>;}struct S;impl P for S{fn f()->impl core::fmt::Display{7u32}}fn main(){}",
            false,
        ),
        (
            "wrong_item",
            "trait P{fn f()->impl Iterator<Item=u32>;}struct S;impl P for S{fn f()->impl Iterator<Item=u8>{0..2}}fn main(){}",
            false,
        ),
        (
            "wrong_concrete",
            "trait P{fn f()->impl Iterator<Item=u32>;}struct S;impl P for S{fn f()->u32{7}}fn main(){}",
            false,
        ),
        (
            "lost_clone",
            "trait P{fn f()->impl Iterator<Item=u32>+Clone;}struct S;struct I;impl Iterator for I{type Item=u32;fn next(&mut self)->Option<u32>{None}}impl P for S{fn f()->impl Iterator<Item=u32>{I}}fn main(){}",
            false,
        ),
        (
            "hidden_type",
            "trait P{fn f()->impl Iterator<Item=u32>;}struct S;impl P for S{fn f()->impl Iterator<Item=u32>{0..2}}fn main(){let _:core::ops::Range<u32>=S::f();}",
            false,
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
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .arg("mir")
                .arg(&path),
            scratch.path(),
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        assert_eq!(
            actual.status.success(),
            accepted,
            "flux {name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
}
