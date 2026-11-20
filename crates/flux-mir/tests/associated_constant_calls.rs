//! Associated constant calls and their owner constraints.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn associated_constant_calls_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-associated-calls-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "generic",
            "fn id<T>(x:T)->T{x}struct H<T>(T);impl<T> H<T>{const F:fn(T)->T=id::<T>;}fn f<T>(x:T)->T{H::<T>::F(x)}fn main(){assert_eq!(f(3),3);}",
            true,
        ),
        (
            "trait_constant",
            "trait C{const F:fn(u32)->u32;}struct H;impl C for H{const F:fn(u32)->u32=|x|x+1;}fn f<T:C>()->u32{T::F(2)}fn main(){assert_eq!(f::<H>(),3);}",
            true,
        ),
        (
            "static_output",
            "static X:u32=3;fn get()->&'static u32{&X}struct H;impl H{const F:fn()->&'static u32=get;}fn f()->&'static u32{H::F()}fn main(){}",
            true,
        ),
        (
            "static_owner_type_argument",
            include_str!("../../../tests/run/static_associated_constant_owner.rs"),
            true,
        ),
        (
            "named_owner_type_argument",
            include_str!("../../../tests/run/named_associated_constant_owner.rs"),
            true,
        ),
        (
            "trait_constant_requires_static_owner",
            "trait Type{const NAME:&'static str;} struct S<'a>(&'a str); impl Type for S<'static>{const NAME:&'static str=\"static\";} fn bad<'a>(_: &'a str)->&'static str{<S<'a> as Type>::NAME}fn main(){}",
            false,
        ),
        (
            "reference_constant_requires_static_pointee",
            r#"
                trait Signature { const VALUE: &'static str; }
                struct S<'a>(&'a str);
                impl Signature for S<'static> { const VALUE: &'static str = "s"; }
                impl<T: Signature + ?Sized> Signature for &T {
                    const VALUE: &'static str = T::VALUE;
                }
                fn f<'a>(_: &'a str) -> &'static str {
                    <&'a S<'a> as Signature>::VALUE
                }
                fn main() {}
            "#,
            false,
        ),
        (
            "trait_constant_requires_equal_owner_lifetimes",
            r#"
                trait Signature { const VALUE: &'static str; }
                struct S<'a, 'b>(&'a str, &'b str);
                impl<'a> Signature for S<'a, 'a> { const VALUE: &'static str = "s"; }
                fn f<'a, 'b>(_: &'a str, _: &'b str) -> &'static str {
                    <S<'a, 'b> as Signature>::VALUE
                }
                fn main() {}
            "#,
            false,
        ),
        (
            "trait_constant_requires_adt_outlives",
            r#"
                trait Signature { const VALUE: &'static str; }
                struct S<'a: 'b, 'b>(&'a str, &'b str);
                impl<'a, 'b> Signature for S<'a, 'b> { const VALUE: &'static str = "s"; }
                fn f<'a, 'b>(_: &'a str, _: &'b str) -> &'static str {
                    <S<'a, 'b> as Signature>::VALUE
                }
                fn main() {}
            "#,
            false,
        ),
        (
            "trait_constant_requires_impl_outlives",
            r#"
                trait Signature { const VALUE: &'static str; }
                struct S<'a, 'b>(&'a str, &'b str);
                impl<'a: 'b, 'b> Signature for S<'a, 'b> { const VALUE: &'static str = "s"; }
                fn f<'a, 'b>(_: &'a str, _: &'b str) -> &'static str {
                    <S<'a, 'b> as Signature>::VALUE
                }
                fn main() {}
            "#,
            false,
        ),
        (
            "trait_constant_result_depends_on_owner_lifetime",
            r#"
                trait Signature { const VALUE: Self; }
                struct S<'a>(&'a str);
                impl<'a> Signature for S<'a> { const VALUE: Self = S("s"); }
                fn f<'a>(_: &'a str) -> &'static str {
                    <S<'a> as Signature>::VALUE.0
                }
                fn main() {}
            "#,
            false,
        ),
        (
            "lifetime_dependent_owner_bound",
            "use core::marker::PhantomData; trait Bound<'a> {} impl Bound<'static> for dyn Fn() + 'static {} struct Gate<T:?Sized,U>(PhantomData<T>,PhantomData<U>); impl<T:Bound<'static>+?Sized,U> Gate<T,U> { const N:u32=1; } fn bad<'a>(_: &'a u8) { let _ = Gate::<dyn Fn() + 'a, &'static u8>::N; } fn main() {}",
            false,
        ),
        (
            "unexpected_owner_lifetime_argument",
            "use core::marker::PhantomData; struct Gate<T:?Sized>(PhantomData<T>); impl<T:?Sized> Gate<T> { const N:u32=1; } fn main() { let _ = Gate::<'static,dyn Fn() + 'static>::N; }",
            false,
        ),
        (
            "borrowed_pointer",
            "fn id(x:u32)->u32{x}struct H;impl H{const F:&'static fn(u32)->u32=&(id as fn(u32)->u32);}fn main(){assert_eq!(H::F(3),3);}",
            true,
        ),
        (
            "returned_borrowed_pointer",
            "fn id(x:u32)->u32{x}static ID:fn(u32)->u32=id;fn get()->&'static fn(u32)->u32{&ID}struct H;impl H{const F:fn()->&'static fn(u32)->u32=get;}fn main(){assert_eq!(H::F()(3),3);}",
            true,
        ),
        (
            "linked_output",
            "fn id(x:&u32)->&u32{x}struct H;impl H{const F:fn(&u32)->&u32=id;}fn f(x:&u32)->&u32{H::F(x)}fn main(){}",
            true,
        ),
        (
            "unsafe_pointer",
            "unsafe fn id(x:u32)->u32{x}struct H;impl H{const F:unsafe fn(u32)->u32=id;}fn main(){assert_eq!(unsafe{H::F(3)},3);}",
            true,
        ),
        (
            "c_pointer",
            "extern \"C\" fn id(x:u32)->u32{x}struct H;impl H{const F:extern \"C\" fn(u32)->u32=id;}fn main(){assert_eq!(H::F(3),3);}",
            true,
        ),
        (
            "owner_bound",
            "trait Bound{}impl Bound for u32{}fn id<T>(x:T)->T{x}struct H<T>(T);impl<T:Bound>H<T>{const F:fn(T)->T=id::<T>;}fn f<T:Bound>(x:T)->T{H::<T>::F(x)}fn main(){}",
            true,
        ),
        (
            "missing_owner_bound",
            "trait Bound{}fn id<T>(x:T)->T{x}struct H<T>(T);impl<T:Bound>H<T>{const F:fn(T)->T=id::<T>;}fn f<T>(x:T)->T{H::<T>::F(x)}fn main(){}",
            false,
        ),
        (
            "noncallable",
            "struct H;impl H{const F:u32=3;}fn main(){H::F();}",
            false,
        ),
        (
            "wrong_argument",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}fn main(){H::F(false);}",
            false,
        ),
        (
            "wrong_output",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}fn main(){let _:bool=H::F(3);}",
            false,
        ),
        (
            "unsafe_outside_block",
            "unsafe fn id(x:u32)->u32{x}struct H;impl H{const F:unsafe fn(u32)->u32=id;}fn main(){H::F(3);}",
            false,
        ),
        (
            "const_call",
            "struct H;impl H{const F:fn(u32)->u32=|x|x;}const X:u32=H::F(3);fn main(){}",
            false,
        ),
        (
            "borrowed_output_escape",
            "fn id(x:&u32)->&u32{x}struct H;impl H{const F:fn(&u32)->&u32=id;}fn f(x:&u32)->&'static u32{H::F(x)}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rust = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(
            format!("/associated-calls/{name}.rs"),
            source,
            Edition::E2024,
        );
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors: Vec<_> = hir
            .diagnostics
            .iter()
            .chain(&typed.diagnostics)
            .map(|d| d.message.clone())
            .collect();
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
