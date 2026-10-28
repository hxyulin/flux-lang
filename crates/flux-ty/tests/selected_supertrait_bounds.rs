//! Impl supertrait proofs retain associated equalities from caller bounds.
use std::process::Command;
#[test]
fn selected_supertrait_bounds_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-selected-supertrait-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cases = [
        (
            "owner_projection_lifetime",
            "trait Access<'a>{type Error;type Variant;}struct V<'a,E>(&'a u32,E);impl<'a>Access<'a> for (){type Error=();type Variant=V<'a,Self::Error>;}fn main(){}",
            true,
        ),
        (
            "owner_projection_two_lifetimes",
            "trait Access<'b>{type Error;type Variant;}struct V<'a,'b,E>(&'a u32,&'b u32,E);impl<'a,'b>Access<'b> for &'a (){type Error=();type Variant=V<'a,'b,Self::Error>;}fn main(){}",
            true,
        ),
        (
            "unconstrained_owner_lifetime",
            "trait Access{type Error;type Variant;}struct V<'a,E>(&'a u32,E);impl<'a>Access for (){type Error=();type Variant=V<'a,Self::Error>;}fn main(){}",
            false,
        ),
        (
            "mixed_callback_binders",
            "struct D<'a,'b>(&'a u32,&'b u32);fn f<'a>(cb:impl for<'b>FnOnce(&mut D<'a,'b>)->u32){}fn main(){}",
            true,
        ),
        (
            "mixed_callback_output",
            "struct D<'a,'b>(&'a u32,&'b u32);fn f<'a>(cb:impl for<'b>FnOnce(&mut D<'a,'b>)->Option<&'a u32>){}fn main(){}",
            true,
        ),
        (
            "callback_missing_output_lifetime",
            "fn f(cb:impl Fn(&u32,&u32)->&u32){}fn main(){}",
            false,
        ),
        (
            "supertrait_impl_projection",
            "trait Bound{}trait Size{type Out;}trait Small:Size<Out=Self::Small>{type Small:Bound;}impl<T:Size>Small for T where T::Out:Bound{type Small=T::Out;}trait User:Small{}struct W<T>(T);impl<T:Small>Size for W<T>{type Out=T::Out;}impl<T:Small>User for W<T>{}fn main(){}",
            true,
        ),
        (
            "supertrait_impl_missing",
            "trait Bound{}trait Size{type Out;}trait Small:Size<Out=Self::Small>{type Small:Bound;}impl<T:Size>Small for T where T::Out:Bound{type Small=T::Out;}trait User:Small{}struct W<T>(T);impl<T:Size>Size for W<T>{type Out=T::Out;}impl<T:Size>User for W<T>{}fn main(){}",
            false,
        ),
    ];
    for (name, source, accepts) in cases {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepts,
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
