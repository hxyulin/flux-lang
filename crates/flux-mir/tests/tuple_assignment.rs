//! Tuple destructuring assignments preserve place and ownership checks.
#[test]
fn tuple_assignment_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-tuple-assignment-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("uninitialized", "fn main(){let a;let b;(a,b)=(1u32,2u32);assert_eq!(a+b,3);}",true),
        ("nested", "fn main(){let mut a=1u32;let mut b=2u32;let mut c=3u32;((a,b),c)=((c,a),b);assert_eq!((a,b,c),(3,1,2));}",true),
        ("immutable", "fn main(){let a=1u32;let mut b=2u32;(a,b)=(3,4);}",false),
        ("wrong_arity", "fn main(){let mut a=1u32;let mut b=2u32;(a,b)=(3,4,5);}",false),
        ("shared", "fn main(){let mut x=1u32;let r=&x;let mut y=2u32;(*r,y)=(3,4);}",false),
        ("double_move", "fn main(){let mut a=String::from(\"a\");let mut b=String::from(\"b\");(a,b)=(a,a);}",false),
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
