//! Generic lifetime-bearing methods preserve associated-output and caller contracts.
use std::process::Command;

#[test]
fn generic_lifetime_methods_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-serde-methods-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "visitor_seed",
            include_str!("../../../tests/run/serde_lifetime_methods.rs"),
            true,
        ),
        (
            "static_generic",
            r#"trait R<'a>{fn get(&self)->&'a u32;}trait D<'a>:Sized{fn decode<T:R<'a>>(x:T)->&'a u32{x.get()}}fn main(){}"#,
            true,
        ),
        (
            "local_method_lifetime",
            r#"trait R<'a>:Sized{fn get<'b>(&'b self,x:&'a u32)->&'b Self{let _=x;self}}fn main(){}"#,
            true,
        ),
        (
            "unsized_method_param",
            r#"trait R<'a>:Sized{fn get<T:?Sized>(&self,x:&'a T)->&'a T{x}}fn main(){}"#,
            true,
        ),
        (
            "associated_generic",
            r#"trait R<'a>:Sized{type Out;fn get<T:Q<'a>>(x:T)->T::Out{x.get()}}trait Q<'a>:Sized{type Out;fn get(self)->Self::Out;}fn main(){}"#,
            true,
        ),
        (
            "escape",
            r#"trait Q<'a>:Sized{type Out;fn get(self,x:&'a u32)->Self::Out;}fn bad<'a,T:Q<'a,Out=&'a u32>>(t:T,x:&'a u32)->&'static u32{t.get(x)}fn main(){}"#,
            false,
        ),
        (
            "wrong_trait_lifetime",
            r#"trait Q<'a>{fn get(&self)->&'a u32;}trait R<'a>:Sized{fn get<T:Q<'static>>(x:T)->&'static u32{x.get()}}fn bad<'a,T:Q<'a>>(x:T)->&'static u32{<() as R>::get(x)}impl<'a>R<'a>for(){}fn main(){}"#,
            false,
        ),
        (
            "impl_strengthening",
            r#"trait R<'a>:Sized{fn get<T>(x:T);}impl<'a>R<'a>for(){fn get<T:Copy>(_:T){}}fn main(){}"#,
            false,
        ),
        (
            "wrong_output",
            r#"trait R<'a>:Sized{type Out;fn get<T>(&self,x:T)->Self::Out;}impl<'a>R<'a>for(){type Out=u32;fn get<T>(&self,_:T)->bool{true}}fn main(){}"#,
            false,
        ),
        (
            "borrowed_default_escape",
            r#"trait R<'a>:Sized{fn get<T>(&self,_:T)->&'a u32{&42}fn bad<T>(&self,_:T)->&'static Self{self}}fn main(){}"#,
            false,
        ),
        (
            "bound_equality",
            r#"trait Q<'a>:Sized{type Out;fn get(self)->Self::Out;}trait R<'a>:Sized{fn get<T:Q<'a,Out=&'a str>>(x:T)->&'a str{x.get()}}fn main(){}"#,
            true,
        ),
        (
            "bound_equality_escape",
            r#"trait Q<'a>:Sized{type Out;fn get(self)->Self::Out;}trait R<'a>:Sized{fn get<T:Q<'a,Out=&'a str>>(x:T)->&'static str{x.get()}}fn main(){}"#,
            false,
        ),
        (
            "bound_equality_wrong",
            r#"trait Q<'a>:Sized{type Out;fn get(self)->Self::Out;}trait R<'a>:Sized{fn get<T:Q<'a,Out=u32>>(x:T)->bool{x.get()}}fn main(){}"#,
            false,
        ),
        (
            "owned_noncopy",
            "trait R<'a>:Sized{fn get(self)->&'a u8{panic!()}}fn main(){}",
            true,
        ),
        (
            "named_method_lifetime",
            "trait R<'a>:Copy{fn get<'b>(self,x:&'b u8)->&'b u8{x}}fn main(){}",
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
