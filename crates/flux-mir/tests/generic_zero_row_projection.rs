//! Generic selected outputs need independent proof of lifetime-free contracts.
#[test]
fn generic_zero_row_selections_match_rustc() {
    let wrapper = "trait Has{type Ty:?Sized;}struct ReadOnly<T:?Sized>(T);struct Cast;trait SizeEq<T:?Sized>{type Cast;}impl<T:?Sized>SizeEq<T>for ReadOnly<T>{type Cast=Cast;}";
    let forward = "trait Bound{fn value()->u32;}trait Key{type Out:Bound;}trait Inner{type I;}";
    let mut mismatches = Vec::new();
    for (name, source, accepted) in [
        ("selected_output", include_str!("../../../tests/run/selected_output_bounds.rs").to_owned(), true),
        ("wrapper_alias", format!("{wrapper}fn f<T:Has>(){{let _=std::mem::size_of::<<ReadOnly<T::Ty>as SizeEq<T::Ty>>::Cast>();}}fn main(){{}}"), true),
        ("different_inputs", format!("{wrapper}fn f<T:Has,U:Has>(){{let _=std::mem::size_of::<<ReadOnly<T::Ty>as SizeEq<U::Ty>>::Cast>();}}fn main(){{}}"), false),
        ("selected_static_bound", format!("{forward}impl<T:Inner+'static>Key for T where T::I:Key{{type Out=<T::I as Key>::Out;}}fn f<T:Inner>()where T::I:Key{{let _=<<T as Key>::Out as Bound>::value();}}fn main(){{}}"), false),
        ("nested_static_bound", format!("{forward}trait Check{{}}impl<T:'static>Check for T{{}}impl<T:Inner+Check>Key for T where T::I:Key{{type Out=<T::I as Key>::Out;}}fn f<T:Inner>()where T::I:Key{{let _=<<T as Key>::Out as Bound>::value();}}fn main(){{}}"), false),
        ("ranked_selected_bound", format!("{forward}trait Life<'a>{{}}impl<T>Life<'static>for T{{}}impl<T:Inner>Key for T where for<'a>T:Life<'a>,T::I:Key{{type Out=<T::I as Key>::Out;}}fn f<T:Inner>()where T::I:Key{{let _=<<T as Key>::Out as Bound>::value();}}fn main(){{}}"), false),
        ("declared_output_membership", "trait Mark{}trait Has{type Ty:Mark;}struct ReadOnly<T:?Sized>(T);struct Cast;trait SizeEq<T:?Sized>{type Cast;}impl<T:?Sized+Mark>SizeEq<T>for ReadOnly<T>{type Cast=Cast;}fn f<T:Has>(){let _=std::mem::size_of::<<ReadOnly<T::Ty>as SizeEq<T::Ty>>::Cast>();}fn main(){}".into(), true),
        ("selected_static_input", "fn id<T>(x:T)->T{x}trait Ref{type Out;}impl<'a>Ref for &'a u8{type Out=&'a u8;}fn f<'a>(x:&'a u8)->&'static u8{id::<<&'static u8 as Ref>::Out>(x)}fn main(){}".into(), false),
    ] {
        let root=std::env::temp_dir().join(format!("flux-generic-zero-row-{name}-{}",std::process::id()));std::fs::create_dir_all(&root).unwrap();let path=root.join("case.rs");std::fs::write(&path,&source).unwrap();
        let reference=std::process::Command::new("rustc").args(["--edition=2024","--emit=metadata","--out-dir"]).arg(&root).arg(&path).output().unwrap();assert_eq!(reference.status.success(),accepted,"rustc {name}: {}",String::from_utf8_lossy(&reference.stderr));
        let(_,_,hir)=flux_hir::lower_text(&path,&source);assert!(hir.diagnostics.is_empty(),"{name}: {:?}",hir.diagnostics);let typed=flux_mir::check_file(&hir);let mut errors=typed.diagnostics.clone();if errors.is_empty(){errors.extend(flux_mir::build_file(&hir,&typed).diagnostics);}
        if errors.is_empty()!=accepted{mismatches.push(format!("{name}: {errors:?}"));}std::fs::remove_dir_all(root).unwrap();
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
