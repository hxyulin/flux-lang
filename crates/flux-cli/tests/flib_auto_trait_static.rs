//! Imported conditional marker impls retain their static prerequisites.
use std::path::Path;
use std::process::{Command, Output};

fn success(output: Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn imported_static_prerequisites_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-flib-auto-static-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let library = root.join("marker.rs");
    std::fs::write(
        &library,
        "pub struct Guard<T>(pub T);impl<T:'static> Unpin for Guard<T>{}pub trait Needs<T:Unpin>{}",
    )
    .unwrap();
    let reference = root.join("libmarker.rlib");
    success(
        Command::new("rustc")
            .args(["--edition=2024", "--crate-type=rlib", "--crate-name=marker"])
            .arg(&library)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap(),
        "rustc dependency",
    );
    let metadata = root.join("marker.flib");
    let compiler = Path::new(env!("CARGO_BIN_EXE_fluxc"));
    success(
        Command::new(compiler)
            .arg("build")
            .arg(&library)
            .args([
                "--crate-type=lib",
                "--crate-name=marker",
                "--metadata-only",
                "-o",
            ])
            .arg(&metadata)
            .output()
            .unwrap(),
        "Flux dependency",
    );
    for (name, source, accepted) in [
        (
            "call_static",
            "pub fn f<T:'static>(x:T){need(marker::Guard(x));}fn need<T:Unpin>(_:T){}",
            true,
        ),
        (
            "call_borrowed",
            "pub fn f<'a>(x:&'a u32){need(marker::Guard(x));}fn need<T:Unpin>(_:T){}",
            false,
        ),
        (
            "trait_argument_static",
            "pub fn f<'a:'static,X:marker::Needs<marker::Guard<&'a u32>>>(){}",
            true,
        ),
        (
            "trait_argument_borrowed",
            "pub fn f<'a,X:marker::Needs<marker::Guard<&'a u32>>>(){}",
            false,
        ),
        (
            "method_static",
            "pub trait M{fn f<T:'static>();}impl M for(){fn f<T:'static>()where marker::Guard<T>:Unpin{}}",
            true,
        ),
        (
            "method_stricter",
            "pub trait M{fn f<T>();}impl M for(){fn f<T>()where marker::Guard<T>:Unpin{}}",
            false,
        ),
    ] {
        let source_path = root.join(format!("{name}.rs"));
        std::fs::write(&source_path, source).unwrap();
        let rust = Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--extern",
            ])
            .arg(format!("marker={}", reference.display()))
            .arg(&source_path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let flux = Command::new(compiler)
            .arg("build")
            .arg(&source_path)
            .args(["--crate-type=lib", "--metadata-only", "--extern"])
            .arg(format!("marker={}", metadata.display()))
            .arg("-o")
            .arg(root.join(format!("{name}.flib")))
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepted,
            "Flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
