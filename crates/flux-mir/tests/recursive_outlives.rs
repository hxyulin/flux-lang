//! Recursive storage retains generic and projected outlives obligations.
#[test]
fn recursive_outlives_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-recursive-outlives-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("same_valid", "struct Link<T>{next:*const Link<T>,value:T}fn borrow<'a,T>(x:&'a Link<T>)->&'a T{&x.value}fn main(){}",true),
        ("same_escape", "struct Link<T>{next:*const Link<T>,value:T}fn static_value<T:'static>(_:T){}fn escape<'a>(x:&'a u32){static_value(Link{next:std::ptr::null(),value:x});}fn main(){}",false),
        ("growing_valid", "struct Next<T>(*const T);struct Link<T>{next:*const Link<Next<T>>,value:T}fn borrow<'a,T>(x:&'a Link<T>)->&'a T{&x.value}fn main(){}",true),
        ("growing_escape", "struct Next<T>(*const T);struct Link<T>{next:*const Link<Next<T>>,value:T}fn static_value<T:'static>(_:T){}fn escape<'a>(x:&'a u32){static_value(Link{next:std::ptr::null(),value:x});}fn main(){}",false),
        ("nested_projected_valid", "trait Value{type Stored:Value;}struct R<T:Value>(Box<G<T>>,T::Stored);struct G<T:Value>(Box<R<T::Stored>>);struct Pair<T:Value>(R<T>,G<T>);fn borrow<'a,T:Value>(x:&'a Pair<T>)->&'a Pair<T>{x}fn main(){}",true),
        ("nested_projected_escape", "trait Value{type Stored:Value;}struct R<T:Value>(Box<G<T>>,T::Stored);struct G<T:Value>(Box<R<T::Stored>>);struct Pair<T:Value>(R<T>,G<T>);fn static_value<T:'static>(_:T){}fn escape<T:Value>(x:Pair<T>){static_value(x);}fn main(){}",false),
    ] {
        let path=root.join(format!("{name}.rs")); std::fs::write(&path,source).unwrap();
        let oracle=std::process::Command::new("rustc").args(["--edition=2024","--emit=metadata"]).arg(&path).arg("-o").arg(root.join("oracle.rmeta")).output().unwrap();
        assert_eq!(oracle.status.success(),accepted,"{name}: {}",String::from_utf8_lossy(&oracle.stderr));
        let (_,_,hir)=flux_hir::lower_text(&path,source);
        let mut errors=hir.diagnostics.clone();
        if errors.is_empty() { let typed=flux_mir::check_file(&hir); errors.extend(typed.diagnostics.clone()); if errors.is_empty(){errors.extend(flux_mir::build_file(&hir,&typed).diagnostics);} }
        assert_eq!(errors.is_empty(),accepted,"{name}: {errors:?}");
    }
}
