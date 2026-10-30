//! Checked slice windows and pointer helpers used by the current backend graph.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn pointer_slice_helpers_match_rustc() {
    support::oracle::compare_fixture("pointer_slice_helpers");
}

#[test]
fn slice_view_borrows_match_rustc() {
    use std::{process::Command, time::Duration};
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-slice-view-errors"));
    for (name, source) in [
        (
            "nested_pointer_variance",
            "struct Holder<T>(core::marker::PhantomData<fn(fn(T))>);fn widen<'a>(x:Holder<&'a i32>)->Holder<&'static i32>{x}fn main(){}",
        ),
        (
            "nested_callback_escape",
            "fn apply<T>(step:fn(T)->T,value:T)->T{step(value)}fn identity<T>(value:T)->T{value}fn run<T>(callback:fn(fn(T)->T,T)->T,step:fn(T)->T,value:T)->T{callback(step,value)}fn escape<'a>()->&'a i32{let value=3;run(apply::<&i32>,identity::<&i32>,&value)}fn main(){}",
        ),
        (
            "unsized_move",
            "fn main(){let mut value=core::mem::ManuallyDrop::new([1u8,2]);let view:&mut core::mem::ManuallyDrop<[u8]>=&mut value;let moved=**view;}",
        ),
        (
            "chunk_alias",
            "fn main(){let mut data=[1,2,3];let first=data.first_chunk_mut::<1>().unwrap();let last=data.last_chunk_mut::<1>().unwrap();first[0]=last[0];}",
        ),
        (
            "chunk_write",
            "fn main(){let mut data=[1,2,3];let first=data.first_chunk::<1>().unwrap();data[0]=4;println!(\"{}\",first[0]);}",
        ),
        (
            "iterator_alias",
            "fn main(){let mut it=[1,2,3].into_iter();let view=it.as_mut_slice();let _=it.next();view[0]=4;}",
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
        assert!(!oracle.status.success(), "invalid rustc probe {name}");
        assert!(!actual.status.success(), "invalid Flux probe {name}");
    }
}
