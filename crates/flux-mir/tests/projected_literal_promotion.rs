//! Projection preserves the promoted aggregate root, including through Index.
#[test]
fn projected_literals_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-projected-literals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("full", "fn get()->&'static[u32]{&[1u32,2][..]}fn main(){}",true),
        ("empty", "fn get<T>()->&'static[T]{&[][..]}fn main(){}",true),
        ("dynamic_index", "fn get(i:usize)->&'static u32{&[1u32,2][i]}fn main(){}",true),
        ("dynamic_range", "fn get(i:usize)->&'static[u32]{&[1u32,2][i..]}fn main(){}",true),
        ("runtime_root", "fn get(v:u32)->&'static[u32]{&[v,2][..]}fn main(){}",false),
        ("mutable_nonempty", "fn get()->&'static mut[u32]{&mut[1u32,2][..]}fn main(){}",false),
        ("drop_root", "fn get()->&'static[String]{&[String::new()][..]}fn main(){}",false),
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
