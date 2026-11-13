use flux_hir::SourceDb;
use flux_intern::Symbol;
use flux_syntax::Edition;

fn check(source: &str, accepted: bool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "flux-loop-tail-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("case.rs");
    std::fs::write(&path, source).unwrap();
    for (edition, flag) in [
        (Edition::E2021, "--edition=2021"),
        (Edition::E2024, "--edition=2024"),
    ] {
        let reference = std::process::Command::new("rustc")
            .args([flag, "--emit=metadata", "--out-dir"])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "{flag}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let mut db = SourceDb::new();
        let root = db.add_with_edition(&path, source, edition);
        let hir = flux_hir::lower(&mut db, root, Symbol::intern("case"));
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "{flag}: {errors:?}");
    }
}

#[test]
fn tail_borrows_end_at_each_iteration() {
    let prefix = "struct Read<'a>(&'a mut u32);impl Drop for Read<'_>{fn drop(&mut self){}}fn read(x:&mut u32)->Result<Read<'_>,()>{*x+=1;Ok(Read(x))}fn main(){let mut x=0;";
    for body in [
        "while x<3{match read(&mut x){Ok(_)=>{},Err(())=>break}}",
        "while x<3{if true{match read(&mut x){Ok(_)=>{},Err(())=>break}}}",
        "while let true=x<3{match read(&mut x){Ok(_)=>{},Err(())=>break}}",
        "loop{if x==3{break}match read(&mut x){Ok(_)=>{},Err(())=>break}}",
    ] {
        check(&format!("{prefix}{body}}}"), true);
    }
}
#[test]
fn a_borrow_saved_across_iterations_remains_live() {
    check(
        "fn main(){let mut x=0;let mut saved=None;while x<3{if let Some(r)=saved.take(){println!(\"{}\",r);}saved=Some(&mut x);x+=1;}}",
        false,
    );
}
#[test]
fn a_reference_cannot_escape_the_loop_body() {
    check(
        "fn main(){let mut result=None;for _ in 0..1{let value=7;result=Some(&value);}println!(\"{}\",result.unwrap());}",
        false,
    );
}
