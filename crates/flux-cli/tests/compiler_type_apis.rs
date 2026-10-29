//! Required string/OS ownership traits and numeric constants use standard APIs.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn compiler_type_apis_match_rustc_off_and_on() {
    support::oracle::compare_fixture("compiler_type_apis");
}

#[test]
fn ownership_and_comparison_contracts_match_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-type-api-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "no_path_ref_default",
            "fn main(){let _:&std::path::Path=Default::default();}",
        ),
        (
            "no_os_string_string_eq",
            "fn main(){let _=std::ffi::OsString::from(\"x\")==String::from(\"x\");}",
        ),
        (
            "no_string_os_string_eq",
            "fn main(){let _=String::from(\"x\")==std::ffi::OsString::from(\"x\");}",
        ),
        (
            "no_os_str_string_eq",
            "fn main(){let _=*std::ffi::OsStr::new(\"x\")==String::from(\"x\");}",
        ),
        (
            "no_string_os_str_eq",
            "fn main(){let _=String::from(\"x\")==*std::ffi::OsStr::new(\"x\");}",
        ),
        (
            "cow_return_escape",
            "fn f()->std::borrow::Cow<'static,str>{let s=String::from(\"x\");std::borrow::Cow::from(&s)}fn main(){}",
        ),
        (
            "cow_borrowed",
            "fn f<'a>(s:&'a String)->std::borrow::Cow<'a,str>{std::borrow::Cow::from(s)}fn main(){let s=String::from(\"x\");assert_eq!(&*f(&s),\"x\");}",
        ),
        (
            "cow_owned",
            "fn f()->std::borrow::Cow<'static,str>{std::borrow::Cow::from(String::from(\"x\"))}fn main(){assert_eq!(&*f(),\"x\");}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        let (_, _, hir) = flux_hir::lower_text(&path, source);
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
