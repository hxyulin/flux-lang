//! Qualified projections are not alias edges; transitive Self bounds imply Sized.
use std::process::Command;

#[test]
fn projection_alias_cycles_and_sized_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-projection-alias-cycle-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "projection_same_name",
            r#"trait Source{type Out;}type Out<T>=<T as Source>::Out;fn f<T:Source>(x:Out<T>)->T::Out{x}fn main(){}"#,
            true,
        ),
        (
            "pair_names",
            r#"trait Pair{type First;type Second;}type First<T>=<T as Pair>::First;type Second<T>=<T as Pair>::Second;fn f<T:Pair>(x:First<T>,y:Second<T>)->(T::First,T::Second){(x,y)}fn main(){}"#,
            true,
        ),
        ("actual_cycle", r#"type Bad=Bad;fn main(){}"#, false),
        (
            "qualified_self_cycle",
            r#"trait Source{type Out;}type Bad=<Bad as Source>::Out;fn main(){}"#,
            false,
        ),
        ("mutual_cycle", r#"type A=B;type B=A;fn main(){}"#, false),
        (
            "transitive_sized",
            r#"struct W<T>(T);trait A:B{type Out<T>:From<W<Self>>;}trait B:C{}trait C:Sized{}fn main(){}"#,
            true,
        ),
        (
            "where_sized",
            r#"struct W<T>(T);trait A where Self:B{type Out<T>:From<W<Self>>;}trait B:Sized{}fn main(){}"#,
            true,
        ),
        (
            "missing_sized",
            r#"struct W<T>(T);trait A{type Out<T>:From<W<Self>>;}fn main(){}"#,
            false,
        ),
        (
            "trait_arg_cycle",
            "trait Q<T>{type Out;}type Bad=<()as Q<Bad>>::Out;fn main(){}",
            false,
        ),
        (
            "qualified_shadow_cycle",
            "type A<A>=crate::A<A>;fn main(){}",
            false,
        ),
        (
            "plain_parameter_shadow",
            "trait Q{type Out;}type A<A>=<A as Q>::Out;fn main(){}",
            true,
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
