//! Foreign safety qualifiers retain the same call obligations as rustc.
#[test]
fn foreign_safety_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-foreign-safety-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "safe",
            "unsafe extern \"C\" { safe fn abs(x:i32)->i32; } fn main(){let f:extern \"C\" fn(i32)->i32=abs;let _=f(-7);let _=abs(-7);}",
            true,
        ),
        (
            "default_unsafe",
            "unsafe extern \"C\" { fn abs(x:i32)->i32; } fn main(){let _=abs(-7);}",
            false,
        ),
        (
            "explicit_unsafe",
            "unsafe extern \"C\" { unsafe fn abs(x:i32)->i32; } fn main(){let _=abs(-7);}",
            false,
        ),
        (
            "unsafe_reified",
            "unsafe extern \"C\" { fn abs(x:i32)->i32; } fn main(){let _:extern \"C\" fn(i32)->i32=abs;}",
            false,
        ),
        (
            "safe_macro",
            "macro_rules! item {($i:item)=>{$i};} item!(unsafe extern \"C\" {safe fn abs(x:i32)->i32;}); fn main(){let _=abs(-7);}",
            true,
        ),
        (
            "safe_outside_macro",
            "macro_rules! item {()=>{safe fn value(){}}} item!();fn main(){}",
            false,
        ),
        (
            "contradictory",
            "unsafe extern \"C\" {safe unsafe fn abs(x:i32)->i32;} fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
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
        let (db, file, hir) = flux_hir::lower_text(&path, source);
        let mut errors: Vec<_> = db
            .file(file)
            .parse
            .errors()
            .iter()
            .map(|d| d.message.clone())
            .collect();
        errors.extend(hir.diagnostics.iter().map(|d| d.message.clone()));
        if errors.is_empty() {
            errors.extend(
                flux_ty::check_file(&hir)
                    .diagnostics
                    .iter()
                    .map(|d| d.message.clone()),
            );
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
