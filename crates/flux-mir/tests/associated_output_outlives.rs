//! Associated output lifetime bounds remain available in generic bodies.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn associated_output_outlives_match_rustc() {
    let dir = std::env::temp_dir().join(format!(
        "flux-associated-output-outlives-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, source) in [
        (
            "static_output",
            true,
            "trait Tr{type Out:'static;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "inherited_static_output",
            true,
            "trait Base{type Out:'static;}trait Tr:Base{}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "no_output_bound",
            false,
            "trait Tr{type Out;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "owner_static_output",
            true,
            "trait Tr:'static{type Out;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "invalid_static_impl",
            false,
            "trait Tr{type Out:'static;}struct S<'a>(&'a u32);impl<'a> Tr for S<'a>{type Out=&'a u32;}fn main(){}",
        ),
        (
            "nested_static_output",
            true,
            "trait Tr{type Out:'static;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&Option<T::Out>){need(x)}fn main(){}",
        ),
        (
            "unrelated_static_output",
            false,
            "trait Tr<A>{type Out;}fn need<T:'static>(_:&T){}fn test<A:'static,T:Tr<A>>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "output_static_trait",
            true,
            "trait Static: 'static {} trait Tr{type Out:Static;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "output_inherited_static_trait",
            true,
            "trait Static: 'static {}trait Mid:Static{}trait Tr{type Out:Mid;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "unrelated_bound_static",
            false,
            "trait Static<A:'static>{}trait Tr{type Out:Static<u32>;}fn need<T:'static>(_:&T){}fn test<T:Tr>(x:&T::Out){need(x)}fn main(){}",
        ),
        (
            "type_family_outlives",
            true,
            "trait Tr{type Out<T>;}fn test<'a,U:Tr+'a,T:'a>(_:&'a (),x:U::Out<T>)->impl Sized+'a{x}fn main(){}",
        ),
        (
            "family_missing_argument_bound",
            false,
            "trait Tr{type Out<T>;}fn test<'a,U:Tr+'a,T>(_:&'a (),x:U::Out<T>)->impl Sized+'a{x}fn main(){}",
        ),
        (
            "family_missing_owner_bound",
            false,
            "trait Tr{type Out<T>;}fn test<'a,U:Tr,T:'a>(_:&'a (),x:U::Out<T>)->impl Sized+'a{x}fn main(){}",
        ),
        (
            "family_borrowed_nominal",
            true,
            "trait Tr{type Out<T>;}struct Array<T,U:Tr>(U::Out<T>);fn test<'a,U:Tr,T>(_:&'a Array<T,U>,x:U::Out<T>)->impl Sized+'a{x}fn main(){}",
        ),
        (
            "family_const_argument",
            true,
            "trait Tr{type Out<T,const N:usize>;}fn test<'a,U:Tr+'a,T:'a>(_:&'a (),x:U::Out<T,3>)->impl Sized+'a{x}fn main(){}",
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-Awarnings"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let diagnostics = if typed.diagnostics.is_empty() {
            flux_mir::build_file(&hir, &typed).diagnostics
        } else {
            typed.diagnostics
        };
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
