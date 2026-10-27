//! Family arguments are distinct from trait membership and selected impl arguments.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn family_types_and_constants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-gat-types-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "type_family",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=(T,);}fn main(){let _: <S as P>::A<u32>=(7,);}",
            true,
        ),
        (
            "const_family",
            "trait P{type A<const N:usize>;}struct S;impl P for S{type A<const N:usize>=[u8;N];}fn main(){let _: <S as P>::A<3>=[1,2,3];}",
            true,
        ),
        (
            "selected_header",
            "trait P<A>{type F<T>;}struct S<A>(A);impl<A>P<A> for S<A>{type F<T>=(T,A);}fn main(){let _: <S<u8> as P<u8>>::F<u32>=(7u32,9u8);}",
            true,
        ),
        (
            "swapped_header",
            "trait P<A>{type F<T>;}struct S<A,B>(A,B);impl<A,B>P<B> for S<B,A>{type F<T>=(A,T,B);}fn main(){let _: <S<u8,u16> as P<u8>>::F<u32>=(7u16,8u32,9u8);}",
            true,
        ),
        (
            "nested_family",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=(T,);}type X=<S as P>::A<<S as P>::A<u32>>;fn main(){let _:X=((7,),);}",
            true,
        ),
        (
            "family_bound",
            "trait P{type A<T:Clone>:Clone;}struct S;impl P for S{type A<U:Clone>=Vec<U>;}fn main(){let _: <S as P>::A<u32>=vec![7];}",
            true,
        ),
        (
            "where_family_bound",
            "trait P{type A<T>:Clone where T:Clone;}struct S;impl P for S{type A<U>=Vec<U> where U:Clone;}fn main(){}",
            true,
        ),
        (
            "generic_family_projection",
            "trait P{type A<T>:Clone;}fn f<T:P>(x:T::A<u32>)->T::A<u32>{x.clone()}fn main(){}",
            true,
        ),
        (
            "omitted_type_argument",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=T;}fn f(_: <S as P>::A){}fn main(){}",
            false,
        ),
        (
            "extra_type_argument",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=T;}fn f(_: <S as P>::A<u32,u32>){}fn main(){}",
            false,
        ),
        (
            "constant_in_type_slot",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=T;}fn f(_: <S as P>::A<3>){}fn main(){}",
            false,
        ),
        (
            "type_in_constant_slot",
            "trait P{type A<const N:usize>;}struct S;impl P for S{type A<const N:usize>=[u8;N];}fn f(_: <S as P>::A<u32>){}fn main(){}",
            false,
        ),
        (
            "wrong_family_instantiation",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=(T,);}fn main(){let _: <S as P>::A<u32>=(7u8,);}",
            false,
        ),
        (
            "wrong_header_instantiation",
            "trait P<A>{type F<T>;}struct S<A>(A);impl<A>P<A> for S<A>{type F<T>=(T,A);}fn main(){let _: <S<u8> as P<u16>>::F<u32>=(7u32,9u16);}",
            false,
        ),
        (
            "stricter_family_bound",
            "trait P{type A<T>;}struct S;impl P for S{type A<T:Copy>=T;}fn main(){}",
            false,
        ),
        (
            "wrong_family_arity",
            "trait P{type A<T>;}struct S;impl P for S{type A=u32;}fn main(){}",
            false,
        ),
        (
            "wrong_family_kind",
            "trait P{type A<const N:usize>;}struct S;impl P for S{type A<T>=T;}fn main(){}",
            false,
        ),
        (
            "unmet_family_where",
            "trait P{type A<T:Copy>;}struct S;impl P for S{type A<T:Copy>=T;}fn f(_: <S as P>::A<String>){}fn main(){}",
            false,
        ),
        (
            "unmet_family_where_body",
            "trait P{type A<T:Copy>;}struct S;impl P for S{type A<T:Copy>=T;}fn main(){let _: <S as P>::A<String>=String::new();}",
            false,
        ),
        (
            "unsized_argument",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=Box<T>;}fn f(_: <S as P>::A<str>){}fn main(){}",
            false,
        ),
        (
            "unsized_output",
            "trait P{type A<T:?Sized>;}struct S;impl P for S{type A<T:?Sized>=T;}fn main(){}",
            false,
        ),
        (
            "declared_bound_not_implicitly_inherited",
            "trait P{type A<T:Copy>;}struct S;impl P for S{type A<U>=U;}fn main(){}",
            false,
        ),
        (
            "trait_owner_shadow",
            "trait P<T>{type A<T>;}fn main(){}",
            false,
        ),
        (
            "impl_owner_shadow",
            "trait P{type A<U>;}struct S<T>(T);impl<T>P for S<T>{type A<T>=T;}fn main(){}",
            false,
        ),
        (
            "trait_lifetime_shadow",
            "trait P<'a>{type A<'a>;}fn main(){}",
            false,
        ),
        (
            "impl_lifetime_shadow",
            "trait P{type A<'a>;}struct S<'b>(&'b u32);impl<'b>P for S<'b>{type A<'b>=u32;}fn main(){}",
            false,
        ),
        (
            "family_iterator_equality",
            "trait P{type A<T>:Iterator<Item=T>;}fn f<X:P>(mut x:X::A<u8>)->u8{x.next().unwrap()}fn main(){}",
            true,
        ),
        (
            "family_iterator_wrong_output",
            "trait P{type A<T>:Iterator<Item=T>;}fn f<X:P>(mut x:X::A<u8>)->u16{x.next().unwrap()}fn main(){}",
            false,
        ),
        (
            "family_iterator_impl_equality",
            "trait P{type A<T>:Iterator<Item=T>;}struct S;impl P for S{type A<T>=std::vec::IntoIter<T>;}fn main(){}",
            true,
        ),
        (
            "family_iterator_impl_wrong_equality",
            "trait P{type A<T>:Iterator<Item=T>;}struct S;impl P for S{type A<T>=std::vec::IntoIter<u32>;}fn main(){}",
            false,
        ),
        (
            "family_where_equality",
            "trait B{type X;}trait P{type A<T:B<X=u32>>;}struct S;impl P for S{type A<U:B<X=u32>>=U;}fn main(){}",
            true,
        ),
        (
            "family_where_stricter_equality",
            "trait B{type X;}trait P{type A<T:B>;}struct S;impl P for S{type A<U:B<X=u32>>=U;}fn main(){}",
            false,
        ),
        (
            "family_where_missing_equality",
            "trait B{type X;}trait P{type A<T:B<X=u32>>;}struct S;impl P for S{type A<U:B>=U;}fn main(){}",
            false,
        ),
        (
            "direct_family_cycle",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=Self::A<T>;}fn main(){}",
            false,
        ),
        (
            "mutual_family_cycle",
            "trait P{type A<T>;type B<T>;}struct S;impl P for S{type A<T>=Vec<Self::B<T>>;type B<T>=Self::A<T>;}fn main(){}",
            false,
        ),
        (
            "growing_family_cycle",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=Self::A<(T,T)>;}fn main(){}",
            false,
        ),
        (
            "abstract_family_identity",
            "trait P{type A<T>;}fn f<X:P>(x:X::A<u8>)->X::A<u8>{x}fn main(){}",
            true,
        ),
        (
            "generic_family_inference",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=(T,);}fn take<T>(x:<S as P>::A<T>)->T{x.0}fn main(){let x:u32=take((7,));assert_eq!(x,7);}",
            true,
        ),
        (
            "body_family_placeholder",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=(T,);}fn main(){let x:<S as P>::A<_>=(7u8,);let _:u8=x.0;}",
            true,
        ),
        (
            "dyn_generic_family",
            "trait P{type A<T>;}fn f(_: &dyn P){}fn main(){}",
            false,
        ),
        (
            "dyn_sized_opt_out",
            "trait P{type A<T> where Self:Sized;fn n(&self)->u32;}struct S;impl P for S{type A<T>=T;fn n(&self)->u32{7}}fn main(){let x:&dyn P=&S;assert_eq!(x.n(),7);}",
            true,
        ),
        (
            "abstract_family_mismatch",
            "trait P{type A<T>;}fn f<X:P>(x:X::A<u8>)->X::A<u32>{x}fn main(){}",
            false,
        ),
        (
            "abstract_const_mismatch",
            "trait P{type A<const N:usize>;}fn f<X:P>(x:X::A<2>)->X::A<3>{x}fn main(){}",
            false,
        ),
        (
            "unmet_output_bound",
            "trait P{type A<T>:Clone;}struct S;impl P for S{type A<T>=Vec<T>;}fn main(){}",
            false,
        ),
        (
            "omitted_family_lifetime",
            "trait P{type A<'a> where Self:'a;}struct S;impl P for S{type A<'a>=u8 where Self:'a;}fn main(){let _:<S as P>::A=7;}",
            false,
        ),
        (
            "omitted_lifetime_before_type",
            "trait P{type A<'a,T> where Self:'a;}struct S;impl P for S{type A<'a,T>=T where Self:'a;}fn main(){let _:<S as P>::A<u8>=7;}",
            false,
        ),
        (
            "explicit_anonymous_lifetime",
            "trait P{type A<'a> where Self:'a;}struct S;impl P for S{type A<'a>=u8 where Self:'a;}fn main(){let _:<S as P>::A<'_>=7;}",
            true,
        ),
        (
            "weakened_implicit_sized",
            "trait P{type A<T>;}struct S;impl P for S{type A<T:?Sized>=Box<T>;}fn main(){}",
            false,
        ),
        (
            "explicit_sized_restores_contract",
            "trait P{type A<T>;}struct S;impl P for S{type A<T:?Sized>=Box<T> where T:Sized;}fn main(){}",
            true,
        ),
        (
            "declared_sized_allows_default",
            "trait P{type A<T:?Sized> where T:Sized;}struct S;impl P for S{type A<T>=T;}fn main(){}",
            true,
        ),
        (
            "declared_copy_allows_default",
            "trait P{type A<T:?Sized> where T:Copy;}struct S;impl P for S{type A<T:Copy>=T;}fn main(){}",
            true,
        ),
        (
            "dyn_indirect_sized_opt_out",
            "trait Marker:Sized{}trait P{type A<T> where Self:Marker;fn n(&self){}}fn f(_: &dyn P){}fn main(){}",
            true,
        ),
        (
            "dyn_copy_opt_out",
            "trait P{type A<T> where Self:Copy;fn n(&self){}}fn f(_: &dyn P){}fn main(){}",
            true,
        ),
        (
            "family_local_projected_output",
            "trait B{type X;}trait P{type A<T:B>:Iterator<Item=T::X>;}struct S;impl P for S{type A<T:B>=std::vec::IntoIter<T::X>;}fn main(){}",
            true,
        ),
        (
            "family_output_not_in_method_scope",
            "trait B{type X;}trait P{type A<T:B>:Iterator<Item=T::X>;fn n(&self){}}struct S;impl P for S{type A<T:B>=std::vec::IntoIter<T::X>;}fn main(){}",
            true,
        ),
        (
            "unsized_parameter_at_family_use",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=u8;}fn f<T:?Sized>(_:<S as P>::A<T>){}fn main(){}",
            false,
        ),
        (
            "sized_where_at_family_use",
            "trait P{type A<T>;}struct S;impl P for S{type A<T>=u8;}fn f<T:?Sized>(_:<S as P>::A<T>)where T:Sized{}fn main(){}",
            true,
        ),
        (
            "unsized_family_parameter",
            "trait P{type A<T:?Sized>;}struct S;impl P for S{type A<T:?Sized>=Box<T>;}fn f<T:?Sized>(_:<S as P>::A<T>){}fn main(){}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
