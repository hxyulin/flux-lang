//! Only complete Send/Sync cycles are coinductive; every leaf still holds.
#[test]
fn recursive_auto_traits_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-recursive-auto-traits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("recursive", "struct N{next:Option<Box<N>>,n:u32}fn need<T:Send+Sync>(){}fn main(){need::<N>();}",true),
        ("generic_recursive", "struct N<T>{next:Option<Box<N<T>>>,value:T}fn need<T:Send+Sync>(){}fn generic<T:Send+Sync>(){need::<N<T>>();}fn main(){generic::<u32>();}",true),
        ("mutual", "struct A(Option<Box<B>>);struct B(Option<Box<A>>,u32);fn need<T:Send+Sync>(){}fn main(){need::<A>();}",true),
        ("static", "use std::sync::OnceLock;struct N(Option<Box<N>>,u32);static ROOT:OnceLock<N>=OnceLock::new();fn main(){}",true),
        ("unsafe_cell_leaf", "struct N(Option<Box<N>>,std::cell::UnsafeCell<u32>);fn need<T:Sync>(){}fn main(){need::<N>();}",false),
        ("raw_leaf", "struct N(Option<Box<N>>,*const u32);fn need<T:Send>(){}fn main(){need::<N>();}",false),
        ("rc_leaf", "struct N(Option<Box<N>>,std::rc::Rc<u32>);fn need<T:Send>(){}fn main(){need::<N>();}",false),
        ("inductive_cycle", "struct N(Option<Box<N>>);trait Gate{}impl Gate for N where N:Send{}unsafe impl Send for N where N:Gate{}fn need<T:Send>(){}fn main(){need::<N>();}",false),
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
