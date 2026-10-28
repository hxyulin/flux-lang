//! Generic lifetime-bearing methods preserve associated-output and caller contracts.
use std::process::Command;

#[test]
fn trait_argument_bounds_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-trait-arguments-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "concrete_bad",
            r#"trait Error{}trait R<E:Error>{}fn f<T:R<u32>>(_:T){}fn main(){}"#,
            false,
        ),
        (
            "generic_missing",
            r#"trait Error{}trait R<E:Error>{}fn f<E,T:R<E>>(_:T){}fn main(){}"#,
            false,
        ),
        (
            "generic_good",
            r#"trait Error{}trait R<E:Error>{}fn f<E:Error,T:R<E>>(_:T){}fn main(){}"#,
            true,
        ),
        (
            "struct_bad",
            r#"trait Error{}trait R<E:Error>{}struct S<T:R<u32>>(T);fn main(){}"#,
            false,
        ),
        (
            "impl_bad",
            r#"trait Error{}trait R<E:Error>{}impl R<u32> for(){}fn main(){}"#,
            false,
        ),
        (
            "super_bad",
            r#"trait Error{}trait R<E:Error>{}trait S:R<u32>{}fn main(){}"#,
            false,
        ),
        (
            "output_bad",
            r#"trait Error{}trait R<E:Error>{}trait S{type Out:R<u32>;}fn main(){}"#,
            false,
        ),
        (
            "owner_good",
            r#"trait Error{}trait R<E:Error>{}trait S<E:Error>{fn f<T:R<E>>(_:T);}fn main(){}"#,
            true,
        ),
        (
            "projection_good",
            r#"trait Error{}struct Good;impl Error for Good{}trait Source{type Out;}trait R<E:Error>{}fn f<X:Source<Out=Good>,T:R<X::Out>>(_:T){}fn main(){}"#,
            true,
        ),
        (
            "projection_bad",
            r#"trait Error{}trait Source{type Out;}trait R<E:Error>{}fn f<X:Source<Out=u32>,T:R<X::Out>>(_:T){}fn main(){}"#,
            false,
        ),
        (
            "nested_bad",
            r#"trait Error{}trait P<U:Error>{}trait R<E:P<U>,U:Error>{}fn f<T:R<(),u32>>(_:T){}fn main(){}"#,
            false,
        ),
        (
            "separate_args",
            r#"trait A{}trait B{}trait R<X:A,Y:B>{}fn f<X:A,Y:B,T:R<X,Y>>(_:T){}fn main(){}"#,
            true,
        ),
        (
            "dyn_formation",
            "trait Error{}trait R<E:Error>{fn f(&self);}fn f(_: &dyn R<u32>){}fn main(){}",
            true,
        ),
        (
            "dyn_good",
            "trait Error{}impl Error for(){}trait R<E:Error>{fn f(&self);}fn f(_: &dyn R<()>){}fn main(){}",
            true,
        ),
        (
            "nested_fn_pointer",
            "trait Error{}trait R<E:Error>{fn f(&self);}fn f(_: fn(&dyn R<u32>)){}fn main(){}",
            true,
        ),
        (
            "body_object_formation",
            "trait Error{}trait R<E:Error>{fn f(&self);}fn f(){let _:Option<&dyn R<u32>>=None;}fn main(){}",
            true,
        ),
        (
            "output_good",
            "trait Error{}trait R<E:Error>{}trait S<E:Error>{type Out:R<E>;}fn main(){}",
            true,
        ),
        (
            "impl_good",
            "trait Error{}trait R<E:Error>{}impl<E:Error> R<E> for(){}fn main(){}",
            true,
        ),
        (
            "owner_method_good",
            "trait Error{}trait R<E:Error>{}struct S<E:Error>(E);impl<E:Error>S<E>{fn f<T:R<E>>(_:T){}}fn main(){}",
            true,
        ),
        (
            "dyn_method_bad",
            "trait Error{}trait R<E:Error>{fn f(&self);}fn f(x:&dyn R<u32>){x.f()}fn main(){}",
            false,
        ),
        (
            "bad_default",
            "trait Error{}trait R<E:Error=u32>{}fn main(){}",
            false,
        ),
        (
            "good_default",
            "trait Error{}impl Error for(){}trait R<E:Error=()>{}fn main(){}",
            true,
        ),
        (
            "dependent_default",
            "trait Error{}trait R<E:Error,F:Error=E>{}fn main(){}",
            true,
        ),
        (
            "lifetime_owner_missing",
            "trait Error{}trait R<'a,E:Error>{}fn f<T:R<'static,u32>>(_:T){}fn main(){}",
            false,
        ),
        (
            "lifetime_owner_good",
            "trait Error{}trait R<'a,E:Error>{}fn f<'a,E:Error,T:R<'a,E>>(_:T){}fn main(){}",
            true,
        ),
        (
            "borrowed_equality_wrong",
            "trait Q<'d>{type Error;}struct V;impl<'d>Q<'d>for V{type Error=&'static u32;}trait R<'d,E>{type Out:Q<'d,Error=E>;}impl<'d,'a>R<'d,&'a u32>for(){type Out=V;}fn main(){}",
            false,
        ),
        (
            "borrowed_equality_good",
            "trait Q<'d>{type Error;}struct V<'a>(&'a u32);impl<'d,'a>Q<'d>for V<'a>{type Error=&'a u32;}trait R<'d,E>{type Out:Q<'d,Error=E>;}impl<'d,'a>R<'d,&'a u32>for V<'a>{type Out=V<'a>;}fn main(){}",
            true,
        ),
        (
            "default_self_borrowed",
            "trait Error{}struct S<'a>(&'a u32);impl<'a>Error for S<'a>{}trait Q<'a>{type Error;}impl<'a>Q<'a>for S<'a>{type Error=S<'a>;}trait R<'a,E:Error=Self>:Error{type Out:Q<'a,Error=E>;}impl<'a>R<'a>for S<'a>{type Out=S<'a>;}fn main(){}",
            true,
        ),
        (
            "default_self_wrong_region",
            "trait Error{}struct S<'a>(&'a u32);impl<'a>Error for S<'a>{}trait Q<'a>{type Error;}impl<'a>Q<'a>for S<'a>{type Error=S<'static>;}trait R<'a,E:Error=Self>:Error{type Out:Q<'a,Error=E>;}impl<'a>R<'a>for S<'a>{type Out=S<'a>;}fn main(){}",
            false,
        ),
        (
            "maybe_unsized_bad",
            "trait R<T:Sized>{}fn f<E:?Sized,T:R<E>>(_:T){}fn main(){}",
            false,
        ),
        (
            "blanket_sized_bad",
            "trait Error{}impl<T>Error for T{}trait R<E:?Sized+Error>{}fn f<E:?Sized,T:R<E>>(_:T){}fn main(){}",
            false,
        ),
        (
            "maybe_unsized_good",
            "trait R<T:?Sized>{}fn f<E:?Sized,T:R<E>>(_:T){}fn main(){}",
            true,
        ),
        (
            "implicit_sized_bad",
            "trait R<T>{}fn f<E:?Sized,T:R<E>>(_:T){}fn main(){}",
            false,
        ),
        (
            "default_self_deferred",
            "trait R<T=Self>{}fn main(){}",
            true,
        ),
        (
            "default_unsized_deferred",
            "trait R<T:?Sized,U=T>{}fn main(){}",
            true,
        ),
        (
            "default_bound_deferred",
            "trait Error{}trait R<T,U:Error=T>{}fn main(){}",
            true,
        ),
        (
            "default_concrete_unsized",
            "trait R<T=str>{}fn main(){}",
            false,
        ),
        (
            "default_use_bad",
            "trait Error{}trait R<T,U:Error=T>{}fn f<X:R<u32>>(_:X){}fn main(){}",
            false,
        ),
        (
            "dyn_implicit_sized_formation",
            "trait R<T>{fn f(&self);}fn f(_: &dyn R<str>){}fn main(){}",
            true,
        ),
        (
            "dyn_implicit_sized_call",
            "trait R<T>{fn f(&self);}fn f(x:&dyn R<str>){x.f()}fn main(){}",
            false,
        ),
        (
            "family_bound_good",
            "trait Error{}trait R<E:Error>{}trait S{type Out<E:Error>:R<E>;}fn main(){}",
            true,
        ),
        (
            "family_bound_missing",
            "trait Error{}trait R<E:Error>{}trait S{type Out<E>:R<E>;}fn main(){}",
            false,
        ),
        (
            "family_const_and_type",
            "trait Error{}trait R<E:Error>{}trait S{type Out<const N:usize,E:Error>:R<E>;}fn main(){}",
            true,
        ),
        (
            "family_projection_good",
            "trait Error{}struct Good;impl Error for Good{}trait Source{type Out;}trait R<E:Error>{}trait S{type Out<E:Source<Out=Good>>:R<E::Out>;}fn main(){}",
            true,
        ),
        (
            "family_projection_bad",
            "trait Error{}trait Source{type Out;}trait R<E:Error>{}trait S{type Out<E:Source<Out=u32>>:R<E::Out>;}fn main(){}",
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
