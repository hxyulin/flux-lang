//! Closed inline constants own a constant evaluation frame and static storage.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn inline_const_frames_match_rustc() {
    support::oracle::compare_fixture("inline_const_frames");
}

#[test]
fn inline_const_evaluation_waits_for_instantiation() {
    use std::process::Command;
    use std::time::Duration;
    let scratch = support::scratch::Scratch::new("out/inline-const-phases");
    let root = scratch.path();
    for (name, source, accepted) in [
        (
            "unused_overflow",
            "fn bad()->u8{const{let x=255u8;x+1}}fn main(){}",
            false,
        ),
        (
            "unused_generic_overflow",
            "fn bad<T>()->u8{const{let x=255u8;x+1}}fn main(){}",
            true,
        ),
        (
            "unused_panic",
            "fn bad<T>()->u8{const{panic!(\"unused\")}}fn main(){}",
            true,
        ),
        (
            "used_overflow",
            "fn bad()->u8{const{let x=255u8;x+1}}fn main(){let _=bad();}",
            false,
        ),
        (
            "used_panic",
            "fn bad<T>()->u8{const{panic!(\"used\")}}fn main(){let _=bad::<u32>();}",
            false,
        ),
        (
            "named_constant",
            "const fn bad()->u8{const{panic!(\"named\")}}const X:u8=bad();fn main(){let _=X;}",
            false,
        ),
        (
            "local_escape",
            "fn bad()->&'static u8{const{let x=7;&x}}fn main(){let _=bad();}",
            false,
        ),
        (
            "interior_borrow",
            "fn bad()->&'static std::cell::Cell<u8>{const{&std::cell::Cell::new(7)}}fn main(){let _=bad();}",
            false,
        ),
        (
            "recursive",
            "const fn bad()->u8{const{bad()}}fn main(){let _=bad();}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_fluxc")] {
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.args([
                    "--edition=2024",
                    "-C",
                    "panic=abort",
                    "-C",
                    "overflow-checks=off",
                ]);
            } else {
                command.arg("build");
            }
            let output = support::process::run(
                command.arg(&path).arg("-o").arg(root.join("program")),
                root,
                name,
                Duration::from_secs(60),
            )
            .completed();
            let errors = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                accepted,
                "{name} {compiler}: {errors}"
            );
            if !accepted && compiler != "rustc" {
                assert!(errors.contains("error:"), "{name}: {errors}");
                assert!(
                    !errors.contains("panicked at") && !errors.contains("stack overflow"),
                    "{name}: {errors}"
                );
            }
        }
    }
}

#[test]
fn imported_inline_constants_keep_storage_and_deferral() {
    use std::process::Command;
    use std::time::Duration;
    let scratch = support::scratch::Scratch::new("out/imported-inline-const");
    let root = scratch.path();
    let library = root.join("frames.rs");
    std::fs::write(&library, "const fn twice(x:u32)->u32{x*2}pub fn values<T>()->&'static[u32]{const{&[twice(7),twice(11)]}}pub fn first<'a:'a>(x:&'a u8)->&'a u8{x}pub fn forward<'s>(x:&'s u8)->&'s u8{first::<'s>(x)}pub fn unused<T>()->u8{const{panic!(\"deferred\")}} ").unwrap();
    let consumer = root.join("consumer.rs");
    std::fs::write(&consumer, "fn main(){let x=frames::values::<u8>();let y=frames::values::<String>();assert_eq!(*frames::forward(&17),17);println!(\"{:?} {:?}\",x,y);}").unwrap();
    let mut expected = None;
    for compiler in ["rustc", env!("CARGO_BIN_EXE_fluxc")] {
        let is_rustc = compiler == "rustc";
        let artifact = root.join(if is_rustc {
            "libframes.rlib"
        } else {
            "frames.flib"
        });
        let mut build = Command::new(compiler);
        if is_rustc {
            build.args(["--edition=2024", "--crate-type=rlib", "--crate-name=frames"]);
        } else {
            build.args(["build", "--crate-type=lib", "--crate-name=frames"]);
        }
        let output = support::process::run(
            build.arg(&library).arg("-o").arg(&artifact),
            root,
            "library",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "library");
        let program = root.join(if is_rustc {
            "rustc-program"
        } else {
            "flux-program"
        });
        let mut build = Command::new(compiler);
        if is_rustc {
            build.arg("--edition=2024");
        } else {
            build.arg("build");
        }
        let output = support::process::run(
            build
                .arg(&consumer)
                .arg("--extern")
                .arg(format!("frames={}", artifact.display()))
                .arg("-o")
                .arg(&program),
            root,
            "consumer",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "consumer");
        let output = support::process::run(
            &mut Command::new(&program),
            root,
            "run",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "run");
        if let Some(expected) = &expected {
            assert_eq!(&output.stdout, expected);
        } else {
            expected = Some(output.stdout);
        }
    }
}
