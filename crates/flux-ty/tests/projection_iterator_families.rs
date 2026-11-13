//! Guarded projection normalization must retain rigid family keys during inference.
#[test]
fn iterator_and_family_controls_match_rustc() {
    let family = "trait Mode{type Output<T>;}struct Check;impl Mode for Check{type Output<T>=();}";
    for (name, text, accepted) in [
        ("native", include_str!("../../../tests/run/projection_iterator_families.rs").to_owned(), true),
        ("rigid_family_mismatch", format!("{family}fn f<M:Mode>(x:M::Output<u8>)->M::Output<u16>{{x}}fn main(){{}}"), false),
        ("erased_family", format!("{family}fn f(x:<Check as Mode>::Output<u8>)-><Check as Mode>::Output<u16>{{x}}fn main(){{}}"), true),
        ("abstract_family_keys", format!("{family}fn call<M:Mode,T>(_:M::Output<T>){{}}fn f<M:Mode>(x:M::Output<u8>){{call::<M,_>(x)}}fn main(){{}}"), true),
        ("erased_unconstrained", format!("{family}fn call<T>(_:<Check as Mode>::Output<T>){{}}fn main(){{call(());}}"), false),
        ("erased_known_application", format!("{family}fn call<T>(_:<Check as Mode>::Output<T>){{}}fn main(){{let x:<Check as Mode>::Output<u8>=();call(x);}}"), false),
        ("fixed_unconstrained", "trait Mode{type Output<T>;}struct Fixed;impl Mode for Fixed{type Output<T>=u8;}fn call<T>(_:<Fixed as Mode>::Output<T>){}fn main(){let x:<Fixed as Mode>::Output<u16>=7;call(x);}".into(), false),
        ("not_iterators", "struct Not;fn need<I:Iterator>(){}fn main(){need::<std::iter::Chain<Not,Not>>();}".into(), false),
        ("competing_items", "fn need<I:Iterator<Item=u8>>(){}fn main(){need::<std::iter::Chain<std::array::IntoIter<u8,1>,std::array::IntoIter<u16,1>>>();}".into(), false),
    ] {
        let root=std::env::temp_dir().join(format!("flux-iterator-family-{name}-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();let path=root.join("case.rs");std::fs::write(&path,&text).unwrap();
        let reference=std::process::Command::new("rustc").args(["--edition=2024","--emit=metadata","--out-dir"]).arg(&root).arg(&path).output().unwrap();
        assert_eq!(reference.status.success(),accepted,"rustc {name}: {}",String::from_utf8_lossy(&reference.stderr));
        let(db,_,hir)=flux_hir::lower_text(&path,&text);let typed=flux_ty::check_file(&hir);
        let success=db.syntax_errors().is_empty()&&hir.diagnostics.is_empty()&&typed.diagnostics.is_empty();
        assert_eq!(success,accepted,"flux {name}: {:?} {:?} {:?}",db.syntax_errors(),hir.diagnostics,typed.diagnostics);
        std::fs::remove_dir_all(root).unwrap();
    }
}
