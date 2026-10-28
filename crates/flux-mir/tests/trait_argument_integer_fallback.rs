//! Generic lifetime-bearing methods preserve associated-output and caller contracts.
use std::process::Command;

#[test]
fn trait_argument_integer_fallback_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-integer-fallback-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "typed_target",
            r#"fn main(){let x:u16=10000.try_into().expect("conversion");assert_eq!(x,10000);}"#,
            true,
        ),
        (
            "comparison",
            r#"fn main(){let x=2000u16;assert!(x>999.try_into().expect("conversion"));}"#,
            true,
        ),
        (
            "try_from",
            r#"fn main(){let x=u16::try_from(10000).expect("conversion");assert_eq!(x,10000);}"#,
            true,
        ),
        (
            "result_target",
            r#"fn main(){let x:Result<u16,_>=10000.try_into();assert_eq!(x.unwrap(),10000);}"#,
            true,
        ),
        (
            "unique_source",
            r#"struct Target;impl TryFrom<u64>for Target{type Error=();fn try_from(_:u64)->Result<Self,()>{Ok(Target)}}fn main(){let _:Target=10.try_into().unwrap();}"#,
            true,
        ),
        (
            "ambiguous_target",
            r#"fn main(){let _x=10.try_into();}"#,
            false,
        ),
        (
            "wrong_target",
            r#"fn main(){let _:String=10.try_into().unwrap();}"#,
            false,
        ),
        (
            "overflow_source",
            r#"fn main(){let _:u16=300u8.try_into().unwrap();}"#,
            false,
        ),
    ];
    for (name, source, accepts) in cases {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            assert!(!accepts, "{name}: {:?}", typed.diagnostics);
            continue;
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            typed.diagnostics.is_empty() && mir.diagnostics.is_empty(),
            accepts,
            "{name}: types={:?}, MIR={:?}",
            typed.diagnostics,
            mir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
