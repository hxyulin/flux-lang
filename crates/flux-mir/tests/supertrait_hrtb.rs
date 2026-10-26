//! Declaration proofs instantiate higher-ranked assumptions, never static-only ones.
use std::process::Command;

#[test]
fn callback_supertrait_declarations_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-supertrait-hrtb-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, parent, child, accepts) in [
        (
            "mut_input",
            "FnMut(&mut u32)->bool",
            "FnMut(&mut u32)->bool",
            true,
        ),
        ("shared_input", "Fn(&u32)->u32", "Fn(&u32)->u32", true),
        (
            "explicit_binder",
            "for<'a> Fn(&'a u32)->u32",
            "for<'b> Fn(&'b u32)->u32",
            true,
        ),
        (
            "fn_implies_once",
            "FnOnce(&u32)->u32",
            "Fn(&u32)->u32",
            true,
        ),
        (
            "mut_implies_once",
            "FnOnce(&mut u32)",
            "FnMut(&mut u32)",
            true,
        ),
        (
            "two_inputs",
            "FnMut(&u32,&mut u32)",
            "FnMut(&u32,&mut u32)",
            true,
        ),
        (
            "universal_implies_static",
            "FnMut(&'static u32)",
            "FnMut(&u32)",
            true,
        ),
        (
            "static_is_not_universal",
            "FnMut(&u32)",
            "FnMut(&'static u32)",
            false,
        ),
        (
            "static_mut_is_not_universal",
            "FnMut(&mut u32)",
            "FnMut(&'static mut u32)",
            false,
        ),
    ] {
        let source = format!(
            "trait Parent {{}} trait Child:Parent {{}} struct Guard<F>(F); impl<F:{parent}>Parent for Guard<F>{{}} impl<F:{child}>Child for Guard<F>{{}} fn main(){{}}"
        );
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        // Named callback lifetimes retain their existing syntax gate. Inspect
        // the independent declaration proof too, so rejection cannot rely on it.
        if parent.contains("'static") || child.contains("'static") {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .all(|d| d.message == "named lifetimes in `Fn` bounds are not supported yet"),
                "{name}: {:?}",
                typed.diagnostics
            );
        } else {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: {:?}",
                typed.diagnostics
            );
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepts,
            "{name}: {:?}",
            mir.diagnostics
        );
        if !accepts {
            assert!(
                mir.diagnostics.iter().any(|d| d.message
                    == "supertrait lifetime requirements are not satisfied by the impl header"),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
