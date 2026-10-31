//! Structural DST wrappers retain their tail metadata and allocation.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn dst_wrappers_match_rustc_off_and_on() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-dst-wrappers-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["dst_wrappers"] {
        let source = root.join(format!("tests/run/{name}.rs"));
        let reference = dir.join("rustc");
        let output = Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = Command::new(&reference).output().unwrap();
        assert!(expected.status.success());
        for optimize in [false, true] {
            let executable = dir.join(if optimize { "flux-opt" } else { "flux" });
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command.arg("build").arg(&source).arg("-o").arg(&executable);
            if optimize {
                command.arg("-O");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let actual = Command::new(&executable).output().unwrap();
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dst_wrapper_acceptance_matches_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-dst-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, code, accepts) in [
        (
            "str_tail",
            "#[repr(transparent)]struct Text(str);fn f(x:&Text)->&str{&x.0}",
            true,
        ),
        (
            "nested",
            "struct A(B);struct B([u16]);fn f(x:&A)->&[u16]{&x.0.0}",
            true,
        ),
        (
            "generic_tail",
            "struct W<T:?Sized>(T);fn f<T:?Sized>(x:&W<T>)->&T{&x.0}",
            true,
        ),
        (
            "sized_instantiation",
            "struct W<T:?Sized>(T);fn f(x:W<u32>)->u32{x.0}",
            true,
        ),
        ("not_last", "struct Bad{tail:str,flag:u8}", false),
        (
            "generic_not_last",
            "struct Bad<T:?Sized>{tail:T,flag:u8}",
            false,
        ),
        (
            "forward_not_last",
            "struct Bad{tail:Text,flag:u8}struct Text(str);",
            false,
        ),
        ("enum_tail", "enum Bad{A(str)}", false),
        ("sized_parameter", "struct Text(str);fn f(_:Text){}", false),
        (
            "generic_sized_parameter",
            "struct W<T:?Sized>(T);fn f<T:?Sized>(_:W<T>){}",
            false,
        ),
        (
            "sized_return",
            "struct Text(str);fn f()->Text{loop{}}",
            false,
        ),
        (
            "sized_bound",
            "struct Text(str);fn need<T>(){}fn f(){need::<Text>();}",
            false,
        ),
        (
            "sized_local",
            "struct Text(str);fn f(x:&Text){let y=*x;}",
            false,
        ),
        (
            "borrowed_construction",
            "struct Text(str);fn f(x:&str){let y=&Text(*x);}",
            false,
        ),
        (
            "thin_to_wide",
            "struct Text(str);fn f(x:*const u8)->*const Text{x as *const Text}",
            false,
        ),
        (
            "wide_cast",
            "struct Text(str);fn f(x:*const str)->*const Text{x as *const Text}",
            true,
        ),
        (
            "generic_metadata_cast",
            "struct W<T:?Sized>(T);fn f<T:?Sized>(x:*const T)->*const W<T>{x as *const W<T>}",
            true,
        ),
        (
            "different_metadata",
            "struct W<T:?Sized>(T);fn f<T:?Sized,U:?Sized>(x:*const W<T>)->*const W<U>{x as *const W<U>}",
            false,
        ),
        (
            "unused_alias",
            "struct W<T>(T);struct Text(str);type Bad=W<Text>;",
            true,
        ),
        (
            "default_sized",
            "struct W<T>(T);struct Text(str);type Bad=W<Text>;fn f(_: &Bad){}",
            false,
        ),
        (
            "forward_sized_argument",
            "struct Bad(Vec<Text>);struct Text(str);",
            false,
        ),
        (
            "generic_sized_bound",
            "struct W<T:?Sized>(T);fn need<T>(){}fn f<T:?Sized>(){need::<W<T>>();}",
            false,
        ),
        (
            "inferred_ref_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(){let w=W{p:5u32,t:[1u8,2]};let x:&W<_,[u8]>=&w;let _:u32=x.p;}",
            true,
        ),
        (
            "inferred_box_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(){let w=Box::new(W{p:5u32,t:[1u8,2]});let x:Box<W<_,[u8]>>=w;let _:u32=x.p;}",
            true,
        ),
        (
            "mismatched_ref_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(){let w=W{p:5u32,t:[1u8,2]};let x:&W<u64,[u8]>=&w;}",
            false,
        ),
        (
            "mismatched_box_prefix",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(){let w=Box::new(W{p:5u32,t:[1u8,2]});let x:Box<W<u64,[u8]>>=w;}",
            false,
        ),
        (
            "raw_wrapper_unsize",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(p:*mut W<u32,[u8;2]>)->*const W<u32,[u8]>{p}",
            true,
        ),
        (
            "raw_wrapper_prefix_mismatch",
            "struct W<P,T:?Sized>{p:P,t:T}fn f(p:*const W<u32,[u8;2]>)->*const W<u64,[u8]>{p}",
            false,
        ),
        (
            "raw_wrapper_mutability",
            "struct W<T:?Sized>(T);fn f(p:*const W<[u8;2]>)->*mut W<[u8]>{p}",
            false,
        ),
        (
            "raw_wrapper_cast",
            "struct W<T:?Sized>(T);fn f(p:*const W<[u8;2]>)->*const W<[u8]>{p as *const W<[u8]>}",
            true,
        ),
        (
            "raw_wrapper_cast_mutability",
            "struct W<T:?Sized>(T);fn f(p:*const W<[u8;2]>)->*mut W<[u8]>{p as *mut W<[u8]>}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = dir.join(format!("{name}.rs"));
        std::fs::write(&source, format!("{code}\nfn main(){{}}")).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-o"])
            .arg(dir.join(format!("{name}.rmeta")))
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("mir")
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn wrapped_trait_objects_fail_explicitly() {
    let dir = std::env::temp_dir().join(format!("flux-dst-vtable-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, code) in [
        ("direct", "struct W(dyn std::fmt::Debug);fn f(_: &W){}"),
        (
            "generic",
            "struct W<T:?Sized>(T);fn f(_: &W<dyn std::fmt::Debug>){}",
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = dir.join(format!("{name}.rs"));
        std::fs::write(&source, format!("{code}\nfn main(){{}}")).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-o"])
            .arg(dir.join(format!("{name}.rmeta")))
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("mir")
            .arg(&source)
            .output()
            .unwrap();
        assert!(!flux.status.success());
        let diagnostic = String::from_utf8_lossy(&flux.stderr);
        assert!(
            diagnostic.contains("trait object tails in structs are not supported yet"),
            "{name}: {diagnostic}"
        );
        assert!(!diagnostic.contains("panicked"), "{name}: {diagnostic}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
