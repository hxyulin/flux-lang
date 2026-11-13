//! A dyn tail's lifetime bound constrains the tail, independently of its prefix.
#[test]
fn structural_object_prefix_and_tail_regions_match_rustc() {
    let prelude = "trait Value{fn value(&self)->u8;}impl Value for u8{fn value(&self)->u8{*self}}struct Borrow<'a>(&'a u8);impl Value for Borrow<'_>{fn value(&self)->u8{*self.0}}struct Header<'a,T:?Sized>{prefix:&'a u8,tail:T}";
    for (name, body, accepted) in [
        (
            "short_prefix_static_tail",
            "fn convert<'a>(x:&'a Header<'a,u8>)->&'a Header<'a,dyn Value+'static>{x}fn main(){let input=7;let header=Header{prefix:&input,tail:9u8};let object=convert(&header);assert_eq!(*object.prefix+object.tail.value(),16);}",
            true,
        ),
        (
            "borrowed_tail_static_escape",
            "fn convert<'a>(x:&'a Header<'a,Borrow<'a>>)->&'a Header<'a,dyn Value+'static>{x}fn main(){}",
            false,
        ),
        (
            "prefix_escape",
            "fn convert<'a>(x:&'a Header<'a,u8>)->&'a Header<'static,dyn Value+'a>{x}fn main(){}",
            false,
        ),
        (
            "borrowed_tail_bound",
            "fn convert<'a>(x:&'a Header<'a,Borrow<'a>>)->&'a Header<'a,dyn Value+'a>{x}fn main(){let input=7;let header=Header{prefix:&input,tail:Borrow(&input)};assert_eq!(convert(&header).tail.value(),7);}",
            true,
        ),
    ] {
        let source = format!("{prelude}{body}");
        let root = std::env::temp_dir().join(format!(
            "flux-structural-regions-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
