//! Associated declaration bounds require an independently proven owning trait.
use std::process::Command;

#[test]
fn selected_output_bounds_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-selected-output-bounds-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "blanket_output",
            r#"trait Size{}trait Key{type Size:Size;}trait Inner{type Inner;}impl<T:Inner>Key for T where T::Inner:Key{type Size=<T::Inner as Key>::Size;}struct Array<S:Size>(S);fn f<T:Inner>(x:Array<<T as Key>::Size>)->Array<<T::Inner as Key>::Size>where T::Inner:Key{x}fn main(){}"#,
            true,
        ),
        (
            "nested_blanket",
            r#"trait Bound{}trait Key{type Out:Bound;}trait Inner{type I;}impl<T:Inner>Key for T where T::I:Key{type Out=<T::I as Key>::Out;}trait Wrap{type Out;}impl<T:Key>Wrap for(T,){type Out=T::Out;}struct Array<S:Bound>(S);fn f<T:Inner>(x:Array<<(T,)as Wrap>::Out>)->Array<<T as Key>::Out>where T::I:Key{x}fn main(){}"#,
            true,
        ),
        (
            "missing_owner",
            r#"trait Bound{}trait Key{type Out:Bound;}struct Array<S:Bound>(S);fn f<T>(_:Array<<T as Key>::Out>){}fn main(){}"#,
            false,
        ),
        (
            "missing_nested",
            r#"trait Bound{}trait Key{type Out:Bound;}trait Inner{type I;}impl<T:Inner>Key for T where T::I:Key{type Out=<T::I as Key>::Out;}struct Array<S:Bound>(S);fn f<T:Inner>(_:Array<<T as Key>::Out>){}fn main(){}"#,
            false,
        ),
        (
            "circular_owner",
            r#"trait Bound{}trait Key{type Out:Bound;}struct S;impl Key for S where <S as Key>::Out:Bound{type Out=();}fn main(){}"#,
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
