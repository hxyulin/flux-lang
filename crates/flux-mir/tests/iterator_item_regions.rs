//! Associated declaration equalities retain the caller's item lifetimes.
#[test]
fn iterator_item_regions_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-iterator-item-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("collect", "fn get<'a>(items:impl IntoIterator<Item=&'a u32>)->Vec<&'a u32>{let mut out=Vec::new();for item in items{out.push(item);}out}fn main(){}",true),
        ("peek", "#[derive(Clone,Copy)]struct V<'a>(&'a u32);fn get<'a>(items:impl IntoIterator<Item=V<'a>>)->Vec<V<'a>>{let mut iter=items.into_iter().peekable();let first=iter.peek().map(|v|v.0);let mut out=Vec::new();for item in iter{out.push(item);}let _=first;out}fn main(){}",true),
        ("declared_bound", "trait S{type Item;type Iter:Iterator<Item=Self::Item>;}fn get<'a,T:S<Item=&'a u32>>(mut iter:T::Iter)->Option<&'a u32>{iter.next()}fn main(){}",true),
        ("wrong_lifetime", "fn get<'a,'b>(items:impl IntoIterator<Item=&'a u32>)->Vec<&'b u32>{let mut out=Vec::new();for item in items{out.push(item);}out}fn main(){}",false),
        ("peek_reference_escape", "fn get<'a>(items:impl IntoIterator<Item=&'a u32>)->&'a &'a u32{let mut iter=items.into_iter().peekable();iter.peek().unwrap()}fn main(){}",false),
        ("wrong_declared_bound", "trait S{type Item;type Iter:Iterator<Item=Self::Item>;}fn get<'a,'b,T:S<Item=&'a u32>>(mut iter:T::Iter)->Option<&'b u32>{iter.next()}fn main(){}",false),
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
