//! Generic lifetime-bearing methods preserve associated-output and caller contracts.
use std::process::Command;

#[test]
fn associated_lifetime_bounds_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-serde-associated-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "variant_pipeline",
            include_str!("../../../tests/run/serde_associated_lifetimes.rs"),
            true,
        ),
        (
            "associated_outlives",
            "trait R<'a>{type Out:'a;fn get(self)->Self::Out;}fn get<'a,T:R<'a>>(t:T)->T::Out{t.get()}fn main(){}",
            true,
        ),
        (
            "associated_reference_arg",
            "trait Q<T>{}trait R<'a>{type Out:Q<&'a u32>;}fn main(){}",
            true,
        ),
        (
            "wrong_owner_lifetime",
            "trait Q<'a>{}struct S;impl Q<'static> for S{}trait R<'a>{type Out:Q<'a>;}impl<'a>R<'a>for(){type Out=S;}fn main(){}",
            false,
        ),
        (
            "wrong_equality",
            "trait Q<'a>{type Error;}impl<'a>Q<'a>for(){type Error=u32;}trait R<'a>{type Out:Q<'a,Error=bool>;}impl<'a>R<'a>for(){type Out=();}fn main(){}",
            false,
        ),
        (
            "associated_escape",
            "trait Q<'a>{fn get(self)->&'a u32;}trait R<'a>:Sized{type Out:Q<'a>;fn out(self)->Self::Out;}fn bad<'a,T:R<'a>>(t:T)->&'static u32{t.out().get()}fn main(){}",
            false,
        ),
        (
            "outlives_escape",
            "trait R<'a>{type Out:'a;}struct S<'b>(&'b u32);impl<'a,'b>R<'a>for S<'b>{type Out=&'b u32;}fn main(){}",
            false,
        ),
        (
            "bound_missing",
            "trait Q<'a>{}trait R<'a>{type Out:Q<'a>;}impl<'a>R<'a>for(){type Out=u32;}fn main(){}",
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
