//! Ordinary aggregate drop uses only regions of fields with destructors.
#[test]
fn aggregate_drop_liveness_matches_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-aggregate-drop-liveness-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "unrelated_owned_field",
            "struct S<'a>{r:&'a mut u32,v:Vec<u32>}fn main(){let mut x=1;let s=S{r:&mut x,v:vec![2]};assert_eq!(*s.r,1);x=3;assert_eq!(x,3);}",
            true,
        ),
        (
            "tuple",
            "fn main(){let mut x=1;let t=(&mut x,vec![2u32]);assert_eq!(*t.0,1);x=3;assert_eq!(x,3);}",
            true,
        ),
        (
            "user_drop",
            "struct S<'a>{r:&'a u32,v:Vec<u32>}impl Drop for S<'_>{fn drop(&mut self){assert_eq!(*self.r,1);}}fn main(){let mut x=1;let s=S{r:&x,v:vec![2]};x=3;}",
            false,
        ),
        (
            "nested_user_drop",
            "struct W<'a>(&'a u32);impl Drop for W<'_>{fn drop(&mut self){assert_eq!(*self.0,1);}}struct S<'a>{w:W<'a>,v:Vec<u32>}fn main(){let mut x=1;let s=S{w:W(&x),v:vec![2]};x=3;}",
            false,
        ),
        (
            "later_use",
            "struct S<'a>{r:&'a u32,v:Vec<u32>}fn main(){let mut x=1;let s=S{r:&x,v:vec![2]};x=3;assert_eq!(*s.r,1);}",
            false,
        ),
        (
            "escape",
            "struct S<'a>{r:&'a u32,v:Vec<u32>}fn main(){let s;{let x=1;s=S{r:&x,v:vec![2]};}assert_eq!(*s.r,1);}",
            false,
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
