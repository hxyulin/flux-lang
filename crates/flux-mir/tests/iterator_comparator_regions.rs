//! Zip/chain/find keeps the regions of the instantiated iterator item.
#[test]
fn iterator_comparator_regions_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-iterator-comparator-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "scalar",
            "fn compare<'a>(a:&'a[u32],b:&'a[u32],x:&'a u32,y:&'a u32)->Option<((&'a u32,&'a u32),usize)>{a.iter().zip(b).zip(0..).chain([((x,y),7)]).find(|((got,want),_)|**got!=**want)}fn main(){assert_eq!(compare(&[1,2],&[1,3],&7,&9).unwrap().1,1);}",
        ),
        (
            "independent",
            "fn compare(a:&[u32],b:&[u32],x:&u32,y:&u32)->bool{a.iter().zip(b).zip(0..).chain([((x,y),7)]).find(|((got,want),_)|**got!=**want).is_some()}fn main(){assert!(compare(&[1],&[2],&7,&9));}",
        ),
        (
            "local_signature",
            "struct Sig{params:Vec<u32>,ret:u32}struct P{range:usize}fn equal(a:u32,b:u32)->bool{a==b}fn compare(input:&Sig,p:&[P])->bool{let norm=|sig:&Sig|Sig{params:sig.params.clone(),ret:sig.ret};let(sig,expected)=(&norm(input),norm(input));let types=sig.params.iter().zip(&expected.params);let ranges=p.iter().map(|p|p.range);types.zip(ranges).chain([((&sig.ret,&expected.ret),7)]).find(|((got,want),_)|!equal(**got,**want)&&**got!=0&&**want!=0).is_some()}fn main(){assert!(!compare(&Sig{params:vec![1],ret:2},&[P{range:7}]));}",
        ),
        (
            "ranges",
            "struct P{range:usize}fn compare<'a>(a:&'a[u32],b:&'a[u32],p:&[P],x:&'a u32,y:&'a u32)->Option<((&'a u32,&'a u32),usize)>{let types=a.iter().zip(b);let ranges=p.iter().map(|p|p.range);types.zip(ranges).chain([((x,y),7)]).find(|((got,want),_)|**got!=**want)}fn main(){assert_eq!(compare(&[1,2],&[1,3],&[P{range:5},P{range:11}],&7,&9).unwrap().1,11);}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert!(errors.is_empty(), "{name}: {errors:?}");
    }
}
