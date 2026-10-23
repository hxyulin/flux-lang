//! Real core guard types must keep the parent and each dereference borrowed.
//! Invalid Rust is checked with rustc metadata only, never executed.
use std::path::Path;
use std::process::Command;

fn errors(path: &Path, text: &str) -> Vec<String> {
    let (db, root, hir) = flux_hir::lower_text(path, text);
    assert!(db.file(root).parse.errors().is_empty());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    if !typed.diagnostics.is_empty() {
        return typed
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect();
    }
    flux_mir::build_file(&hir, &typed)
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

#[test]
fn vec_guard_access_and_reborrows() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/run/refcell_guards.rs");
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(errors(&path, &text), Vec::<String>::new());
}

#[test]
fn guard_borrow_boundaries_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/refcell_guards");
    let output =
        std::env::temp_dir().join(format!("flux-refcell-guards-{}.rmeta", std::process::id()));
    for (name, code, expected) in [
        (
            "shared_mutation",
            "E0596",
            "the trait bound `Ref<Vec<{integer}>>: DerefMut` is not satisfied",
        ),
        (
            "shared_refmut_mutation",
            "E0596",
            "cannot borrow `*shared` as mutable, as it is behind a `&` reference",
        ),
        ("guard_escape", "E0597", "`cell` does not live long enough"),
        (
            "parent_move",
            "E0505",
            "cannot move out of `cell` because it is borrowed",
        ),
        (
            "reference_escape",
            "E0515",
            "cannot return value referencing local variable `guard`",
        ),
        (
            "mut_reference_escape",
            "E0515",
            "cannot return value referencing local variable `guard`",
        ),
        (
            "alias_mutation",
            "E0499",
            "cannot borrow `guard` as mutable more than once at a time",
        ),
        (
            "shared_alias_mutation",
            "E0502",
            "cannot borrow `guard` as mutable because it is also borrowed as immutable",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        assert!(!rustc.status.success(), "rustc accepted {name}");
        assert!(
            stderr.contains(&format!("error[{code}]")),
            "{name}: {stderr}"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(errors(&path, &text), [expected], "{name}");
    }
    let _ = std::fs::remove_file(output);
}
