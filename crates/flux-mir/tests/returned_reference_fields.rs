//! Field assignment through an implicitly dereferenced returned reference.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn returned_reference_fields_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-returned-reference-fields-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "mutable",
            "struct S{n:u32}fn get(x:&mut S)->&mut S{x}fn main(){let mut x=S{n:1};get(&mut x).n+=2;assert_eq!(x.n,3);}",
            true,
        ),
        (
            "option",
            "struct S{n:u32}fn main(){let mut x=Some(S{n:1});x.as_mut().unwrap().n=3;assert_eq!(x.unwrap().n,3);}",
            true,
        ),
        (
            "shared",
            "struct S{n:u32}fn get(x:&S)->&S{x}fn main(){let x=S{n:1};get(&x).n=3;}",
            false,
        ),
        (
            "immutable_borrow",
            "struct S{n:u32}fn get(x:&mut S)->&mut S{x}fn main(){let x=S{n:1};get(&mut x).n=3;}",
            false,
        ),
        (
            "returned_slice",
            "fn main(){let mut x=Some(vec![3u32,5].into_boxed_slice());x.as_deref_mut().unwrap()[0]=7;assert_eq!(x.as_deref().unwrap(),[7,5]);}",
            true,
        ),
        (
            "returned_shared_slice",
            "fn main(){let x=Some(vec![3u32,5].into_boxed_slice());x.as_deref().unwrap()[0]=7;}",
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
