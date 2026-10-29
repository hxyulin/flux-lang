//! Trait and impl methods preserve their early and late lifetime binders.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn method_binders_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-method-binders-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "ignored_alias_input",
            "type Ignore<'a>=u32;trait Tr{fn f<'a>(x:Ignore<'a>);}struct S;impl Tr for S{fn f(_:u32){}}fn main(){}",
            true,
        ),
        (
            "ignored_alias_output",
            "type Ignore<'a>=u32;trait Tr{fn f<'a>()->Ignore<'a>;}struct S;impl Tr for S{fn f()->u32{3}}fn main(){}",
            false,
        ),
        (
            "input_explicit",
            "trait Tr{fn f(x:&u32)->&u32;}struct S;impl Tr for S{fn f<'a>(x:&'a u32)->&'a u32{x}}fn main(){}",
            true,
        ),
        (
            "mixed_order",
            "trait Tr{fn f<'a,'b>(x:&'a u32)->&'b u32;}struct S;impl Tr for S{fn f<'x,'y>(_:&'y u32)->&'x u32{&3}}fn main(){}",
            true,
        ),
        (
            "owner_only_bound",
            "trait Tr<'a>{fn f(x:&u32);}struct S;impl<'a>Tr<'a>for S{fn f<'b>(_:&'b u32)where 'a:'a{}}fn main(){}",
            true,
        ),
        (
            "predicate_early",
            "trait Mark<'a>{}impl<'a>Mark<'a>for u32{}trait Tr{fn f<'a,T:Mark<'a>>(x:T);}struct S;impl Tr for S{fn f<'b,T:Mark<'b>>(_:T){}}fn main(){}",
            true,
        ),
        (
            "projection_output",
            "trait Stream{type Item<'a>;}trait Tr<T:Stream>{fn f<'a>()->T::Item<'a>;}struct S;impl<T:Stream>Tr<T>for S{fn f()->T::Item<'static>{panic!()}}fn main(){}",
            false,
        ),
        (
            "redundant_bound",
            "trait Tr{fn f(x:&u32)->&u32;}struct S;impl Tr for S{fn f<'a>(x:&'a u32)->&'a u32 where 'a:'a{x}}fn main(){}",
            false,
        ),
        (
            "return_redundant",
            "static V:u32=3;trait Tr{fn f<'a>()->&'a u32;}struct S;impl Tr for S{fn f<'b>()->&'b u32 where 'b:'b{&V}}fn main(){}",
            true,
        ),
        (
            "return_same",
            "static V:u32=3;trait Tr{fn f<'a>()->&'a u32;}struct S;impl Tr for S{fn f<'b>()->&'b u32{&V}}fn main(){}",
            true,
        ),
        (
            "return_static_impl",
            "static V:u32=3;trait Tr{fn f<'a>()->&'a u32;}struct S;impl Tr for S{fn f()->&'static u32{&V}}fn main(){}",
            false,
        ),
        (
            "unused_add",
            "trait Tr{fn f();}struct S;impl Tr for S{fn f<'a>(){}}fn main(){}",
            true,
        ),
        (
            "unused_bounded",
            "trait Tr{fn f<'a>();}struct S;impl Tr for S{fn f<'b>()where 'b:'b{}}fn main(){}",
            false,
        ),
        (
            "unused_omit",
            "trait Tr{fn f<'a>();}struct S;impl Tr for S{fn f(){}}fn main(){}",
            true,
        ),
        (
            "unused_same",
            "trait Tr{fn f<'a>();}struct S;impl Tr for S{fn f<'b>(){}}fn main(){}",
            true,
        ),
        (
            "checkpoint_explicit_bound",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset<'b>(&self,_:&'b Checkpoint<&'a[T],&'a[T]>)->usize where 'a:'b{47}}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(&path, source, Edition::E2021);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
