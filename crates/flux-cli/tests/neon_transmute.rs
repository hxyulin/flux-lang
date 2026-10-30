//! Eight-byte NEON and bit reinterpretation match the native Rust implementation.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn neon_and_transmute_execute_like_rustc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir =
        std::env::temp_dir().join(format!("flux-neon-transmute-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["neon_eight", "transmute", "const_field_promotion"] {
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
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
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
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn transmute_and_neon_contexts_match_rustc() {
    let dir =
        std::env::temp_dir().join(format!("flux-neon-transmute-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, declaration, body, accepts) in [
        (
            "const_field",
            "struct S(u8);const C:S=S(17);fn f()->&'static u8{&C.0}",
            "assert_eq!(*f(),17);",
            true,
        ),
        (
            "local_field",
            "struct S(u8);fn f()->&'static u8{let x=S(17);&x.0}",
            "",
            false,
        ),
        (
            "drop_const_field",
            "struct S(u8);impl Drop for S{fn drop(&mut self){}}const C:S=S(17);fn f()->&'static u8{&C.0}",
            "",
            false,
        ),
        (
            "cell_const_field",
            "use std::cell::Cell;struct S(Cell<u8>,u8);const C:S=S(Cell::new(1),17);fn f()->&'static u8{&C.1}",
            "",
            false,
        ),
        (
            "mutable_const_field",
            "struct S(u8);const C:S=S(17);fn f()->&'static mut u8{&mut C.0}",
            "",
            false,
        ),
        (
            "runtime_field",
            "struct S(u8);fn make()->S{S(17)}fn f()->&'static u8{&make().0}",
            "",
            false,
        ),
        (
            "same_size",
            "",
            "let _:i32=unsafe{std::mem::transmute(7u32)};",
            true,
        ),
        (
            "enum_discriminant_size",
            "enum E{A=0,B=1<<40}",
            "let _:u8=unsafe{std::mem::transmute(E::B)};",
            false,
        ),
        (
            "enum_discriminant_valid",
            "enum E{A=0,B=1<<40}",
            "let _:u64=unsafe{std::mem::transmute(E::B)};",
            true,
        ),
        (
            "different_size",
            "",
            "let _:u64=unsafe{std::mem::transmute(7u32)};",
            false,
        ),
        (
            "generic_invalid",
            "fn f<T,U>(x:T)->U{unsafe{std::mem::transmute(x)}}",
            "",
            false,
        ),
        (
            "generic_arrays_invalid",
            "fn f<T>(x:[T;2])->(T,T){unsafe{std::mem::transmute(x)}}",
            "",
            false,
        ),
        (
            "generic_identity",
            "fn f<T>(x:T)->T{unsafe{std::mem::transmute(x)}}",
            "let _=f(3u32);",
            true,
        ),
        (
            "generic_zero_array",
            "fn f<T>(x:[T;0]){let _:()=unsafe{std::mem::transmute(x)};}",
            "f::<u32>([]);",
            false,
        ),
        (
            "generic_same_pointee",
            "fn f<T:?Sized>(x:*const T)->*mut T{unsafe{std::mem::transmute(x)}}",
            "",
            true,
        ),
        (
            "generic_different_pointee",
            "fn f<T:?Sized,U:?Sized>(x:*const T)->*mut U{unsafe{std::mem::transmute(x)}}",
            "",
            false,
        ),
        (
            "generic_thin_pointer",
            "fn f<T>(x:usize)->*mut T{unsafe{std::mem::transmute(x)}}",
            "let _=f::<u32>(4096);",
            true,
        ),
        (
            "generic_wide_pointer",
            "fn f<T:?Sized>(x:*const T)->usize{unsafe{std::mem::transmute(x)}}",
            "",
            false,
        ),
        (
            "transmute_unsafe",
            "",
            "let _:i32=std::mem::transmute(7u32);",
            false,
        ),
        (
            "intrinsic_pointer",
            "",
            "let _:unsafe fn(u32)->i32=std::mem::transmute;",
            false,
        ),
        (
            "valid_bool",
            "const X:bool=unsafe{std::mem::transmute(1u8)};",
            "",
            true,
        ),
        (
            "invalid_bool",
            "const X:bool=unsafe{std::mem::transmute(2u8)};",
            "",
            false,
        ),
        (
            "invalid_char",
            "const X:char=unsafe{std::mem::transmute(0xd800u32)};",
            "",
            false,
        ),
        (
            "invalid_nonzero",
            "const X:std::num::NonZeroU64=unsafe{std::mem::transmute(0u64)};",
            "",
            false,
        ),
        (
            "transient_invalid_reference",
            "const X:u8={let x=2u8;let r: &bool=unsafe{std::mem::transmute(&x as *const u8)};if *r{1}else{0}};",
            "",
            false,
        ),
        (
            "valid_reference_view",
            "const X:u8={let x=1u8;let r: &bool=unsafe{std::mem::transmute(&x as *const u8)};if *r{1}else{0}};",
            "",
            true,
        ),
        (
            "invalid_reference",
            "const X:&u8=unsafe{std::mem::transmute(1usize)};",
            "",
            false,
        ),
        (
            "invalid_function",
            "const X:fn()=unsafe{std::mem::transmute(0usize)};",
            "",
            false,
        ),
        (
            "padding",
            "#[repr(C)]struct S(u8,u32);const X:u64=unsafe{std::mem::transmute(S(1,2))};",
            "",
            false,
        ),
        (
            "pointer_to_integer",
            "static X:u8=1;const P:usize=unsafe{std::mem::transmute(&X as *const u8)};",
            "",
            false,
        ),
        (
            "eight_load_unsafe",
            "",
            "let x=[0u8;8];let _=core::arch::aarch64::vld1_u8(x.as_ptr());",
            false,
        ),
        (
            "eight_store_unsafe",
            "",
            "let mut x=[0u8;8];let v=unsafe{core::arch::aarch64::vld1_u8(x.as_ptr())};core::arch::aarch64::vst1_u8(x.as_mut_ptr(),v);",
            false,
        ),
        (
            "eight_lane_valid",
            "",
            "unsafe{use core::arch::aarch64::*;let _=vget_lane_u64::<0>(vreinterpret_u64_u8(vdup_n_u8(3)));}",
            true,
        ),
        (
            "eight_lane_invalid",
            "",
            "unsafe{use core::arch::aarch64::*;let _=vget_lane_u64::<1>(vreinterpret_u64_u8(vdup_n_u8(3)));}",
            false,
        ),
        (
            "eight_lane_negative",
            "",
            "unsafe{use core::arch::aarch64::*;let _=vget_lane_u64::<-1>(vreinterpret_u64_u8(vdup_n_u8(3)));}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = dir.join(format!("{name}.rs"));
        std::fs::write(&source, format!("{declaration}\nfn main(){{{body}}}")).unwrap();
        // Lane constants are diagnosed when instantiated, so ask rustc for an executable.
        let output = Command::new("rustc")
            .args(["--edition=2024", "-o"])
            .arg(dir.join(format!("{name}-rustc")))
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("build")
            .arg(&source)
            .arg("-o")
            .arg(dir.join(format!("{name}-flux")))
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
