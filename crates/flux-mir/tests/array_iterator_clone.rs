//! Array iterator Clone requires the payload and preserves adapter bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn array_iterator_clone_bounds_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-array-iterator-clone-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "strings",
            "fn main(){let mut it=[String::from(\"a\"),String::from(\"b\")].into_iter();drop(it.next());let other=it.clone();assert_eq!(other.count(),1);}",
            true,
        ),
        (
            "chain",
            "fn get()->impl Iterator<Item=u32>+Clone{let items:&'static[u32]=&[2,3];[Some(1),None].into_iter().flatten().chain(items.iter().copied())}fn main(){let _=get().clone();}",
            true,
        ),
        (
            "not_clone",
            "struct N;fn main(){let it=[N].into_iter();let _=it.clone();}",
            false,
        ),
        (
            "nonclone_chain",
            "struct N;fn main(){let it=[Some(N),None].into_iter().flatten();let _=it.clone();}",
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
