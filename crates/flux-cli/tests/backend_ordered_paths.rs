//! Ordered collection and Unix filename APIs required by the complete backend.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn ordered_collections_and_paths_match_rustc() {
    support::oracle::compare_fixture("backend_ordered_paths");
}

#[test]
fn ordered_range_and_path_failure_contracts() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/backend-ordered-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, accepts) in [
        (
            "range_escape",
            "fn bad<'a>()->&'a i32{let mut m=BTreeMap::new();m.insert(1,2);m.range(..).next().unwrap().1}fn main(){}",
            false,
        ),
        (
            "range_excludes_mutation",
            "fn main(){let mut m=BTreeMap::new();m.insert(1,2);let mut r=m.range(..);m.clear();println!(\"{:?}\",r.next());}",
            false,
        ),
        (
            "range_requires_order",
            "struct K;fn main(){let m:BTreeMap<K,()>=BTreeMap::new();m.range(..);}",
            false,
        ),
        (
            "values_need_no_order",
            "struct K;fn main(){let m:BTreeMap<K,K>=BTreeMap::new();let mut i=m.into_values();assert_eq!(i.len(),0);assert!(i.next_back().is_none());}",
            true,
        ),
        (
            "borrowed_str_range",
            "fn main(){let mut m=BTreeMap::new();m.insert(String::from(\"a\"),1);let r=m.range::<str,_>((Included(\"a\"),Unbounded));assert_eq!(r.count(),1);}",
            true,
        ),
        (
            "set_requires_order",
            "struct K;fn main(){let mut s:BTreeSet<K>=BTreeSet::new();s.pop_first();}",
            false,
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(&source, format!("use std::collections::{{BTreeMap,BTreeSet}};use std::ops::Bound::{{Included,Unbounded}};{body}")).unwrap();
        for flux in [false, true] {
            let mut command = Command::new(if flux {
                env!("CARGO_BIN_EXE_flux")
            } else {
                "rustc"
            });
            if flux {
                command.arg("mir");
            } else {
                command
                    .args(["--edition=2024", "--emit=metadata"])
                    .arg("-o")
                    .arg(directory.join(format!("{name}.rmeta")));
            }
            command.arg(&source);
            let output = support::process::run(
                &mut command,
                &directory,
                &format!("{name}-{flux}"),
                Duration::from_secs(30),
            )
            .completed();
            assert_eq!(
                output.status.success(),
                accepts,
                "{name}, Flux={flux}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    for (name, body, message) in [
        (
            "reversed_range",
            "let mut m=BTreeMap::new();m.insert(1,2);let _=m.range(9..2);",
            "range start is greater than range end",
        ),
        (
            "excluded_range",
            "let mut m=BTreeMap::new();m.insert(1,2);let _=m.range((Excluded(1),Excluded(1)));",
            "range start and end are equal and excluded",
        ),
        (
            "extension_separator",
            "let _=std::path::Path::new(\"file.rs\").with_extension(\"a/b\");",
            "extension cannot contain path separators",
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!(
                "use std::collections::BTreeMap;use std::ops::Bound::Excluded;fn main(){{{body}}}"
            ),
        )
        .unwrap();
        for flux in [false, true] {
            let executable = directory.join(format!("{name}-{flux}"));
            let mut compile = Command::new(if flux {
                env!("CARGO_BIN_EXE_flux")
            } else {
                "rustc"
            });
            if flux {
                compile.arg("build");
            } else {
                compile.args(["--edition=2024", "-Cpanic=abort"]);
            }
            compile.arg(&source).arg("-o").arg(&executable);
            let output = support::process::run(
                &mut compile,
                &directory,
                &format!("build-{name}-{flux}"),
                Duration::from_secs(30),
            )
            .completed();
            support::oracle::success(&output, "runtime failure fixture build");
            let output = support::process::run(
                &mut Command::new(executable),
                &directory,
                &format!("run-{name}-{flux}"),
                Duration::from_secs(30),
            )
            .completed();
            assert!(!output.status.success(), "{name}, Flux={flux} should panic");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(message),
                "{name}, Flux={flux}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
