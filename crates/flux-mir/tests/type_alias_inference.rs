//! Only omitted expression-path arguments infer; explicit arity stays checked.
#[test]
fn omitted_alias_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-alias-inference-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("omitted", "type Items<T>=Vec<T>;fn main(){let mut v=Items::new();v.push(7u8);}", true),
        ("expected", "type Items<T>=Vec<T>;fn main(){let v:Vec<u64>=Items::default();}", true),
        ("nested", "type Base<T,U>=Vec<(T,U)>;type Items<T>=Base<T,u64>;fn main(){let mut v=Items::new();v.push((7u8,11u64));}", true),
        ("enum_variant", "type Answer<T>=Result<T,u64>;fn main(){let v=Answer::Ok(7u8);let _:Answer<u8>=v;}", true),
        ("explicit_too_few", "type Items<T,U>=Vec<(T,U)>;fn main(){let mut v=Items::<u8>::new();v.push((7,11));}", false),
        ("explicit_too_many", "type Items<T>=Vec<T>;fn main(){let v=Items::<u8,u64>::new();}", false),
        ("annotation", "type Items<T>=Vec<T>;fn main(){let v:Items=Vec::<u8>::new();}", false),
        ("fixed_argument", "type Items<T>=Vec<(T,u64)>;fn main(){let mut v=Items::new();v.push((7u8,false));}", false),
        ("unconstrained", "type Items<T>=Vec<T>;fn main(){let v=Items::new();}", false),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc").args(["--edition=2024", "--emit=metadata"])
            .arg(&path).arg("-o").arg(root.join("probe.rmeta")).output().unwrap();
        assert_eq!(oracle.status.success(), accepted, "{name}: {}", String::from_utf8_lossy(&oracle.stderr));
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() { errors.extend(flux_mir::build_file(&hir, &typed).diagnostics); }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
