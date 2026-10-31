//! Primitive value paths coexist with same-named modules without changing imports.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;

#[test]
fn primitive_paths_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-primitive-paths-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "imported",
            "use std::char;fn main(){assert_eq!(char::MAX,'\\u{10ffff}');assert!(char::is_whitespace('\\u{3000}'));assert_eq!(char::from(65u8),'A');println!(\"{}\",char::is_whitespace('x'));}",
        ),
        (
            "module_coexists",
            "mod char{pub const MAX:u8=3;}fn main(){assert_eq!(char::MAX,3);assert!(char::is_whitespace(' '));}",
        ),
        (
            "module_function",
            "mod char{pub fn is_whitespace(_:char)->bool{false}}fn main(){assert!(!char::is_whitespace(' '));}",
        ),
        (
            "qualified_max",
            "fn main(){assert_eq!(std::char::MAX,'\\u{10ffff}');assert_eq!(core::char::MAX,'\\u{10ffff}');}",
        ),
        (
            "module_alias_constant",
            "use std::char as scalar;fn main(){assert_eq!(scalar::MAX,'\\u{10ffff}');}",
        ),
        (
            "nested",
            "mod child{use std::char;pub fn test()->bool{char::is_whitespace('\\u{85}')}}fn main(){assert!(child::test());}",
        ),
        (
            "qualified_not_method",
            "fn main(){let _=std::char::is_whitespace(' ');}",
        ),
        (
            "renamed_not_primitive",
            "use std::char as scalar;fn main(){let _=scalar::is_whitespace(' ');}",
        ),
        (
            "method_not_import",
            "use std::char::is_whitespace;fn main(){let _=is_whitespace(' ');}",
        ),
        (
            "private_module_method",
            "mod char{fn is_whitespace(_:char)->bool{false}}fn main(){let _=char::is_whitespace(' ');}",
        ),
        (
            "struct_shadow",
            "struct char;fn main(){let _=char::is_whitespace(' ');}",
        ),
        (
            "generic_shadow",
            "fn f<char>(){let _=char::is_whitespace(' ');}fn main(){}",
        ),
        (
            "renamed_custom_module",
            "mod char{}use char as scalar;fn main(){let _=scalar::is_whitespace(' ');}",
        ),
    ] {
        let input = root.join(format!("{name}.rs"));
        std::fs::write(&input, source).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        let expected = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&input)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        for optimized in [false, true] {
            let output = root.join(format!("{name}-{optimized}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command.arg("build").arg(&input).arg("-o").arg(&output);
            if optimized {
                command.arg("-O");
            }
            let actual = command.output().unwrap();
            assert_eq!(
                actual.status.success(),
                expected.status.success(),
                "{name}: rustc={} flux={}",
                String::from_utf8_lossy(&expected.stderr),
                String::from_utf8_lossy(&actual.stderr)
            );
            if expected.status.success() {
                let rustc = Command::new(&reference).output().unwrap();
                let flux = Command::new(&output).output().unwrap();
                assert_eq!(flux.status.code(), rustc.status.code(), "{name}");
                assert_eq!(flux.stdout, rustc.stdout, "{name}");
                assert_eq!(flux.stderr, rustc.stderr, "{name}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
