//! Destructuring a slice place does not create an unsized temporary.
#[test]
fn slice_pattern_places_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-slice-pattern-places-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("copy_slice", "fn first(values:&[u32])->u32{let [value]=values[..] else{return 0};value}fn main(){assert_eq!(first(&[7]),7);assert_eq!(first(&[]),0);}",true),
        ("vec_field", "struct S{values:Vec<u32>}fn main(){let s=S{values:vec![7]};let [value]=s.values[..] else{return};assert_eq!(value,7);assert_eq!(s.values[0],7);}",true),
        ("ref_elements", "fn first(values:&[String])->usize{let [ref value]=values[..] else{return 0};value.len()}fn main(){assert_eq!(first(&[String::from(\"owned\")]),5);}",true),
        ("ref_rest", "fn first(values:&[u32])->usize{let [head,ref rest @ ..]=values[..] else{return 0};assert_eq!(head,7);rest.len()}fn main(){assert_eq!(first(&[7,11,29]),2);}",true),
        ("move_noncopy", "fn first(values:&[String]){let [value]=values[..] else{return};drop(value);}fn main(){}",false),
        ("unsized_rest", "fn first(values:&[u32]){let [head,rest @ ..]=values[..] else{return};drop(rest);}fn main(){}",false),
        ("unsized_binding", "fn first(values:&[u32]){let copied=values[..];}fn main(){}",false),
        ("escaping_element", "fn first()->&'static u32{let values=vec![7u32];let [ref value]=values[..] else{panic!()};value}fn main(){}",false),
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
