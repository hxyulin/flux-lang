//! Vectored buffers retain native layout and custom errors retain ownership.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn vectored_buffers_and_custom_errors_match_rustc() {
    support::oracle::compare_fixture("io_vectored_errors");
}

#[test]
fn vectored_and_error_contracts_match_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-io-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let borrowed = r#"#[derive(Debug)]struct Borrowed<'a>(&'a str);impl std::fmt::Display for Borrowed<'_>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(self.0)}}impl std::error::Error for Borrowed<'_>{}"#;
    for (name, source) in [
        (
            "borrowed_error",
            format!(
                "{borrowed}fn f<'a>(s:&'a str)->Box<dyn std::error::Error+'a>{{Borrowed(s).into()}}fn main(){{}}"
            ),
        ),
        (
            "copied_message",
            String::from(
                "fn f(s:&str)->Box<dyn std::error::Error+Send+Sync>{s.into()}fn main(){let s=String::from(\"local\");let e=f(&s);drop(s);assert_eq!(e.to_string(),\"local\");}",
            ),
        ),
        (
            "io_static_payload",
            format!(
                "{borrowed}fn main(){{let s=String::from(\"local\");let _=std::io::Error::new(std::io::ErrorKind::Other,Borrowed(&s));}}"
            ),
        ),
        (
            "io_sync_payload",
            String::from(
                "#[derive(Debug)]struct Bad(std::cell::Cell<u8>);impl std::fmt::Display for Bad{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(\"bad\")}}impl std::error::Error for Bad{}fn main(){let _=std::io::Error::new(std::io::ErrorKind::Other,Bad(std::cell::Cell::new(0)));}",
            ),
        ),
        (
            "error_borrow_escape",
            String::from(
                "fn f()->&'static(dyn std::error::Error+Send+Sync){let e=std::io::Error::new(std::io::ErrorKind::Other,\"x\");e.get_ref().unwrap()}fn main(){}",
            ),
        ),
        (
            "io_slice_escape",
            String::from(
                "fn f()->std::io::IoSlice<'static>{let b=[0u8];std::io::IoSlice::new(&b)}fn main(){}",
            ),
        ),
        (
            "no_io_slice_eq",
            String::from("fn main(){let a=std::io::IoSlice::new(b\"x\");let _=a==a;}"),
        ),
        (
            "io_slice_markers",
            String::from(
                "fn yes<T:Send+Sync+Copy+Clone+std::fmt::Debug>(){}fn main(){yes::<std::io::IoSlice<'static>>();}",
            ),
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let typed = flux_mir::check_file(&hir);
        let mut errors: Vec<_> = hir
            .diagnostics
            .iter()
            .chain(&typed.diagnostics)
            .map(|d| d.message.clone())
            .collect();
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
        assert_eq!(
            errors.is_empty(),
            oracle.status.success(),
            "{name}: rustc={} Flux={errors:?}",
            String::from_utf8_lossy(&oracle.stderr)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
