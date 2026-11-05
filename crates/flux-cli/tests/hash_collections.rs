//! Collision, ownership and borrowing oracles for custom-hasher collections.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::hash;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
extern crate alloc as alloc_crate;
#[path = "../../../library/std/src/collections/hash_map.rs"]
#[allow(unused_imports, reason = "std reexports the test does not use")]
mod hash_map;

#[test]
fn panicking_drops_preserve_collection_counts() {
    use hash_map::DefaultHasher;
    use std::cell::Cell;
    use std::hash::BuildHasherDefault;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    struct Owner<'a>(&'a Cell<bool>);
    impl Drop for Owner<'_> {
        fn drop(&mut self) {
            if self.0.replace(false) {
                panic!("drop once");
            }
        }
    }
    let panic = Cell::new(false);
    for clear in [false, true] {
        let mut map =
            hash_map::HashMap::<usize, Owner<'_>, BuildHasherDefault<DefaultHasher>>::default();
        for key in 0..20 {
            map.insert(key, Owner(&panic));
        }
        panic.set(true);
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                if clear {
                    map.clear();
                } else {
                    map.retain(|_, _| false);
                }
            }))
            .is_err()
        );
        assert_eq!(map.len(), 19);
        assert_eq!(map.iter().count(), map.len());
        assert_eq!(map.iter().len(), map.len());
        for key in 20..40 {
            map.insert(key, Owner(&panic));
        }
        assert_eq!(map.len(), 39);
        map.clear();
        assert!(map.is_empty());
        map.insert(99, Owner(&panic));
        assert!(map.contains_key(&99));
    }
}

fn bounded(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(30)).completed()
}

#[test]
fn custom_hasher_collections_match_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/hash-collections/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = repo.join("tests/run/hash_collections.rs");
    let reference = root.join("reference");
    let built = bounded(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "rustc-build",
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let expected = bounded(&mut Command::new(&reference), &root, "rustc-run");
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let built = bounded(&mut command, &root, &format!("build-{optimized}"));
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let actual = bounded(
            &mut Command::new(&executable),
            &root,
            &format!("run-{optimized}"),
        );
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
