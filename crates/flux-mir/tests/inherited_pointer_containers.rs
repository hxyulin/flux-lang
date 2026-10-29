//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn inherited_pointer_containers_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-inherited-pointer-containers-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "container_fields",
            "struct Value<'a,T>{x:&'a T,f:fn(&T)->u32}struct Iter<'a,T>{x:Option<&'a T>}impl<'a,T> Iterator for Iter<'a,T>{type Item=Value<'a,T>;fn next(&mut self)->Option<Self::Item>{self.x.take().map(|x|Value{x,f:|_|7})}}impl<'a,T> DoubleEndedIterator for Iter<'a,T>{fn next_back(&mut self)->Option<Self::Item>{self.next()}}fn main(){}",
            true,
        ),
        (
            "container_escape",
            "struct Value<'a,T>{x:&'a T,f:fn(&T)->u32}struct Iter<'a,T>{x:Option<&'a T>}impl<'a,T> Iterator for Iter<'a,T>{type Item=Value<'a,T>;fn next(&mut self)->Option<Self::Item>{self.x.take().map(|x|Value{x,f:|_|7})}}impl<'a,T> DoubleEndedIterator for Iter<'a,T>{fn next_back(&mut self)->Option<Self::Item>{self.next()}}fn escape<'a>(i:&mut Iter<'a,u32>)->Value<'static,u32>{i.next_back().unwrap()}fn main(){}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, source);
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
}
