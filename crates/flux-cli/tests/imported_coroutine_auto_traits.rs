//! Checked dependency closure MIR reconstructs generic future storage facts.
use std::process::{Command, Output};
fn success(output: Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn imported_generic_async_storage_controls_send() {
    let root = std::env::temp_dir().join(format!("flux-imported-async-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let library = root.join("dep.rs");
    std::fs::write(&library,r#"
        pub async fn preserve<T>(value:T)->T{std::future::ready(()).await;value}
        pub trait Run{fn run(self);}
        impl<F:std::future::Future+Send> Run for F{fn run(self){drop(self);}}
        pub fn wrapper<T:Send>(value:T){Run::run(async move{std::future::ready(()).await;drop(value)});}
        fn private_work<T:Send>(value:T){Run::run(async move{std::future::ready(()).await;drop(value)});}
        fn private_forward<T:Send>(value:T){private_work(value);}
        pub fn two_hop<T:Send>(value:T){private_forward(value);}
    "#).unwrap();
    let artifact = root.join("dep.flib");
    success(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["build", "--crate-type", "lib", "--crate-name", "dep"])
            .arg(&library)
            .arg("-o")
            .arg(&artifact)
            .output()
            .unwrap(),
    );
    let reference_library = root.join("libdep.rlib");
    success(
        Command::new("rustc")
            .args(["--edition=2024", "--crate-type=rlib", "--crate-name=dep"])
            .arg(&library)
            .arg("-o")
            .arg(&reference_library)
            .output()
            .unwrap(),
    );
    for (name, source, accepted) in [
        (
            "send",
            "fn need<F:std::future::Future+Send>(_:F){}fn main(){need(dep::preserve(7u32));}",
            true,
        ),
        (
            "not_send",
            "fn need<F:std::future::Future+Send>(_:F){}fn main(){need(dep::preserve(std::rc::Rc::new(7u32)));}",
            false,
        ),
        ("private_wrapper", "fn main(){dep::wrapper(7u32);}", true),
        (
            "two_hop_private_wrapper",
            "fn main(){dep::two_hop(11u32);}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("dep={}", reference_library.display()))
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let executable = root.join(name);
        let result = Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("build")
            .arg(&path)
            .arg("--extern")
            .arg(format!("dep={}", artifact.display()))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        if accepted {
            success(Command::new(&executable).output().unwrap());
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
