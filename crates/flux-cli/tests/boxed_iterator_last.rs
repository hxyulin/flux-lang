#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn boxed_last_matches_rustc() {
    support::oracle::compare_fixture("boxed_iterator_last");
}
#[test]
fn boxed_last_lifetimes_match_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-box-last-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, expected) in [
        (
            "borrowed",
            "fn f<'a>(s:&'a[u32])->Option<&'a u32>{Box::new(s.iter()).last()}fn main(){}",
            true,
        ),
        (
            "borrow_escape",
            "fn bad()->&'static u32{let values=[7u32,9];Box::new(values.iter()).last().unwrap()}fn main(){}",
            false,
        ),
        (
            "spoof_attribute",
            "#[rustc_intrinsic]#[rustc_sized_dispatch(0,a,b)]fn __sized_dispatch<T:?Sized>(_:&T){}fn a<T>(_:&T){}fn b<T:?Sized>(_:&T){}fn main(){}",
            false,
        ),
        (
            "metadata_only_spoof",
            "#[rustc_sized_dispatch(0,a,b)]fn f<T:?Sized>(_:&T){}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rust = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            expected,
            "{name}: {}",
            String::from_utf8_lossy(&rust.stderr)
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
        assert_eq!(errors.is_empty(), expected, "{name}: {errors:?}");
    }
}

#[test]
fn ordinary_helper_unwind_ownership() {
    use std::{path::Path, process::Command, time::Duration};
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/boxed-last-host/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = std::fs::read_to_string(repo.join("library/alloc/src/boxed.rs")).unwrap();
    let start = source.find("fn __boxed_last_sized").unwrap();
    let end = source[start..]
        .find("impl<I: Iterator + ?Sized> Iterator for Box<I>")
        .unwrap()
        + start;
    let driver = root.join("host.rs");
    std::fs::write(
        &driver,
        include_str!("fixtures/boxed_iterator_last/panic.rs.txt")
            .replace("__HELPERS__", &source[start..end]),
    )
    .unwrap();
    for optimized in [false, true] {
        let exe = root.join(format!("host-{optimized}"));
        let mut c = Command::new("rustc");
        c.args(["--edition=2024", "-Awarnings"])
            .arg(&driver)
            .arg("-o")
            .arg(&exe);
        if optimized {
            c.arg("-O");
        }
        let built =
            support::process::run(&mut c, &root, "build", Duration::from_secs(60)).completed();
        support::oracle::success(&built, "boxed helper host build");
        let output = support::process::run(
            &mut Command::new(exe),
            &root,
            "run",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "boxed helper host run");
        assert_eq!(output.stdout, b"boxed helper ownership complete\n");
    }
}

#[test]
fn dyn_pointer_boundaries_match_supported_contracts() {
    use std::process::Command;
    let root =
        std::env::temp_dir().join(format!("flux-box-last-boundaries-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, message) in [
        (
            "dyn_method_pointer",
            "fn main(){let _:fn(Box<dyn Iterator<Item=u32>>)->Option<u32>=<Box<dyn Iterator<Item=u32>> as Iterator>::last;}",
            None::<&str>,
        ),
        (
            "dyn_borrowed_item",
            "fn f<'a>(s:&'a[u32])->Option<&'a u32>{let i:Box<dyn Iterator<Item=&'a u32>+'a>=Box::new(s.iter());i.last()}fn main(){}",
            None,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rust = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert!(
            rust.status.success(),
            "{}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let typed = flux_mir::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        match message {
            Some(message) => assert!(
                errors.iter().any(|d| d.message == message),
                "{name}: {errors:?}"
            ),
            None => assert!(errors.is_empty(), "{name}: {errors:?}"),
        }
    }
}
