//! Inspecting an enum tag does not use references in its inactive payload.
#[test]
fn enum_discriminant_liveness_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-enum-tag-liveness-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "fn get(v:&mut Vec<u32>,ok:bool)->Result<&mut Vec<u32>,String>{if ok{Ok(v)}else{Err(String::from(\"bad\"))}}";
    for (name, body, accepted) in [
        (
            "err_reborrow",
            "fn f(v:&mut Vec<u32>,ok:bool){match get(v,ok){Ok(x)=>x.push(1),Err(_)=>v.push(2)}}",
            true,
        ),
        (
            "err_loop",
            "fn f(v:&mut Vec<u32>,ok:bool,words:&[u32]){match get(v,ok){Ok(x)=>{for w in words{x.push(*w)}},Err(_)=>{for w in words{v.push(*w)}}}}",
            true,
        ),
        (
            "bound_err",
            "fn f(v:&mut Vec<u32>,ok:bool){match get(v,ok){Ok(x)=>x.push(1),Err(e)=>{v.push(2);drop(e)}}}",
            true,
        ),
        (
            "err_question",
            "fn f(v:&mut Vec<u32>,ok:bool)->Result<(),String>{match get(v,ok){Ok(x)=>x.push(1),Err(_)=>{for w in [1,2]{v.push(w);Ok::<(),String>(())?;}}}Ok(())}",
            true,
        ),
        (
            "err_mutation",
            "fn f(v:&mut Vec<u32>,ok:bool){match get(v,ok){Ok(x)=>x.push(1),Err(_)=>{*v=vec![2]}}}",
            true,
        ),
        (
            "ok_conflict",
            "fn f(v:&mut Vec<u32>,ok:bool){match get(v,ok){Ok(x)=>{v.push(2);x.push(1)},Err(_)=>()}}",
            false,
        ),
        (
            "ok_shared_conflict",
            "fn f(v:&mut Vec<u32>,ok:bool){match get(v,ok){Ok(x)=>{println!(\"{}\",v.len());x.push(1)},Err(_)=>()}}",
            false,
        ),
        (
            "live_result",
            "fn f(v:&mut Vec<u32>,ok:bool){let r=get(v,ok);v.push(2);drop(r)}",
            false,
        ),
        (
            "payload_escape",
            "fn f()->Result<&'static mut Vec<u32>,String>{let mut v=Vec::new();get(&mut v,true)}",
            false,
        ),
        (
            "inspecting_drop",
            "struct D<'a>(&'a u32);impl Drop for D<'_>{fn drop(&mut self){println!(\"{}\",self.0)}}fn f(){let d;{let x=7;d=Some(D(&x));}drop(d)}",
            false,
        ),
        (
            "payload_mutation_before_tag",
            "fn f(){let mut n=1;let e=Some(&mut n);n=2;let _tag=match e{Some(_)=>1,None=>0};}",
            false,
        ),
        (
            "borrowed_discriminant",
            "fn f(){let r;{let e=Some(7);r=&e;}match r{Some(_)=>(),None=>()}}",
            false,
        ),
    ] {
        let source = format!("{prefix}{body}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
