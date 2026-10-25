//! Implicit mutable reborrows reserve argument loans until call activation.
#[test]
fn implicit_argument_reborrows_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-implicit-reborrows-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("implicit", "fn f(v:&mut Vec<u32>,n:usize){v.push(n as u32)}fn g(v:&mut Vec<u32>){f(v,v.len());}fn main(){}",true),
        ("scalar", "fn f(v:&mut u32,n:u32){*v+=n;}fn g(v:&mut u32){f(v,*v);}fn main(){}",true),
        ("explicit", "fn f(v:&mut Vec<u32>,n:usize){}fn main(){let mut v=vec![1u32];f(&mut v,v.len());}",false),
        ("two_mutable", "fn f(a:&mut u32,b:&mut u32){}fn g(x:&mut u32){f(x,x);}fn main(){}",false),
        ("shared_argument", "fn f(v:&mut Vec<u32>,n:&u32){}fn g(v:&mut Vec<u32>){f(v,&v[0]);}fn main(){}",false),
        ("stored_borrow", "fn f(v:&mut Vec<u32>,n:usize){}fn g(v:&mut Vec<u32>){let r=&v[0];f(v,1);let _=*r;}fn main(){}",false),
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
