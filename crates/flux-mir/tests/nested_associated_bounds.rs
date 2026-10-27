//! Nested selected aliases must prove their own impl predicates.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn nested_associated_bounds_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-nested-associated-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let base = "trait E{}impl E for u32{}trait H{type E:E;type S:S<E=Self::E,C=Self::C>;type C:C<E=Self::E>;}trait S{type E:E;type C:C<E=Self::E>;}trait C{type E:E;}struct Head<T>(T);struct Seg<T>(T);struct Sec<T>(T);impl<T:E>C for Sec<T>{type E=T;}impl<T:E>S for Seg<T>{type E=T;type C=Sec<T>;}";
    for(name,source,accepted)in [
        ("mach_header",format!("{base}impl<T:E>H for Head<T>{{type E=T;type S=Seg<T>;type C=Sec<T>;}}fn main(){{}}"),true),
        ("wrong_endian",format!("{base}impl<T:E>H for Head<T>{{type E=u32;type S=Seg<T>;type C=Sec<T>;}}fn main(){{}}"),false),
        ("nested_selected","trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S;impl Q for S{type A=i32;}impl P for S{type A=<S as Q>::A;}impl C<i32>for S{type N=Self;}fn main(){}".into(),true),
        ("nested_wrong_type","trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S;impl Q for S{type A=i32;}impl P for S{type A=<S as Q>::A;}impl C<u32>for S{type N=Self;}fn main(){}".into(),false),
        ("generic_nested","trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T>Q for S<T>{type A=T;}impl<T>P for S<T>{type A=<Self as Q>::A;}impl<T>C<T>for S<T>{type N=Self;}fn main(){}".into(),true),
        ("bounded_nested","trait B{}impl B for u32{}trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T:B>Q for S<T>{type A=T;}impl<T:B>P for S<T>{type A=<Self as Q>::A;}impl<T:B>C<T>for S<T>{type N=Self;}fn main(){}".into(),true),
        ("static_nested", "trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T:'static>Q for S<T>{type A=T;}impl<T:'static>P for S<T>{type A=<Self as Q>::A;}impl<T:'static>C<T>for S<T>{type N=Self;}fn main(){}".into(),true),
        ("missing_static_nested", "trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T:'static>Q for S<T>{type A=T;}impl<T:'static>P for S<T>{type A=<Self as Q>::A;}impl<T>C<T>for S<T>{type N=Self;}fn main(){}".into(),false),
        ("missing_nested_bound","trait B{}trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T:B>Q for S<T>{type A=T;}impl<T>P for S<T>{type A=<Self as Q>::A;}impl<T>C<T>for S<T>{type N=Self;}fn main(){}".into(),false),
        ("static_selected_escape","trait Q{type A;}trait P{type A;}trait C<T>{type N:P<A=T>;}struct S<T>(T);impl<T:'static>Q for S<T>{type A=T;}impl<T:'static>P for S<T>{type A=<Self as Q>::A;}impl<'a>C<&'a u32>for S<&'a u32>{type N=Self;}fn main(){}".into(),false),
    ]{
        if !case_filter::selected(name){continue;}
        let path=directory.join(format!("{name}.rs"));std::fs::write(&path,&source).unwrap();
        let reference=std::process::Command::new("rustc").args(["--edition=2024","--emit=metadata","--out-dir"]).arg(&directory).arg(&path).output().unwrap();
        assert_eq!(reference.status.success(),accepted,"rustc {name}: {}",String::from_utf8_lossy(&reference.stderr));
        let(_,_,hir)=flux_hir::lower_text(&path,&source);let mut errors:Vec<_>=hir.diagnostics.iter().map(|d|d.message.clone()).collect();
        if errors.is_empty(){let typed=flux_mir::check_file(&hir);errors.extend(typed.diagnostics.iter().map(|d|d.message.clone()));if errors.is_empty(){errors.extend(flux_mir::build_file(&hir,&typed).diagnostics.into_iter().map(|d|d.message));}}
        assert_eq!(errors.is_empty(),accepted,"flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
