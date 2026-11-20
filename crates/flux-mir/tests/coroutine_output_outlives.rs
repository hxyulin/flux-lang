//! A future's output marker does not own a value across suspension.
#[test]
fn coroutine_storage_outlives_controls_match_rustc() {
    for (name, source, accepted) in [
        (
            "output_only",
            "use std::{future::Future,pin::Pin};fn empty<T>()->Pin<Box<dyn Future<Output=T>+Send+'static>>{Box::pin(async{unreachable!()})}fn main(){}",
            true,
        ),
        (
            "output_reference_only",
            "use std::{future::Future,pin::Pin};fn empty<'a>()->Pin<Box<dyn Future<Output=&'a u8>+Send+'static>>{Box::pin(async{unreachable!()})}fn main(){}",
            true,
        ),
        (
            "captured_output",
            "use std::{future::Future,pin::Pin};fn bad<T:Send>(value:T)->Pin<Box<dyn Future<Output=T>+Send+'static>>{Box::pin(async move{value})}fn main(){}",
            false,
        ),
        (
            "captured_reference",
            "use std::{future::Future,pin::Pin};fn bad<'a>(value:&'a u8)->Pin<Box<dyn Future<Output=u8>+Send+'static>>{Box::pin(async move{*value})}fn main(){}",
            false,
        ),
        (
            "await_argument_temporary",
            "async fn length(value:&String)->usize{std::future::ready(()).await;value.len()}async fn forward()->usize{length(&String::from(\"kept\")).await}fn main(){}",
            true,
        ),
        (
            "await_explicit_block_temporary",
            "async fn length(value:&String)->usize{value.len()}async fn forward()->usize{{length(&String::from(\"too short\"))}.await}fn main(){}",
            false,
        ),
    ] {
        let root = std::env::temp_dir().join(format!(
            "flux-coroutine-outlives-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
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
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
