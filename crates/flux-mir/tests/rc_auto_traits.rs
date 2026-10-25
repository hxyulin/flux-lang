//! Rc's raw allocation owner must never acquire Send or Sync structurally.
#[test]
fn rc_auto_traits_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-rc-auto-traits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("plain", "use std::rc::Rc; fn main(){let a=Rc::new(7u32);let b=a.clone();assert_eq!(*b,7);}", true),
        ("send", "use std::rc::Rc;fn send<T:Send>(){} fn main(){send::<Rc<u32>>();}", false),
        ("sync", "use std::rc::Rc;fn sync<T:Sync>(){} fn main(){sync::<Rc<u32>>();}", false),
        ("borrow_send", "use std::rc::Rc;fn send<T:Send>(){} fn main(){send::<&Rc<u32>>();}", false),
        ("borrow_sync", "use std::rc::Rc;fn sync<T:Sync>(){} fn main(){sync::<&Rc<u32>>();}", false),
        ("slice_send", "use std::rc::Rc;fn send<T:Send>(){} fn main(){send::<Rc<[u8]>>();}", false),
        ("slice_sync", "use std::rc::Rc;fn sync<T:Sync>(){} fn main(){sync::<Rc<[u8]>>();}", false),
        ("shared_mut", "use std::rc::Rc;fn main(){let mut a=Rc::new(7u32);let b=a.clone();*a=9;}", false),
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
