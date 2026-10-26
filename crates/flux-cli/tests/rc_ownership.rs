//! Rc ownership, clone panic invariants and native rustc comparisons.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
extern crate alloc as alloc_crate;
use alloc_crate::{alloc, vec};
#[path = "../../../library/alloc/src/rc.rs"]
mod rc;

#[test]
fn clone_panic_preserves_shared_owners() {
    use rc::Rc;
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    struct Owner<'a> {
        panic: &'a Cell<bool>,
        drops: &'a Cell<u32>,
    }
    impl Clone for Owner<'_> {
        fn clone(&self) -> Self {
            assert!(!self.panic.get(), "clone once");
            Owner {
                panic: self.panic,
                drops: self.drops,
            }
        }
    }
    impl Drop for Owner<'_> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let panic = Cell::new(false);
    let drops = Cell::new(0);
    let mut a = Rc::new(Owner {
        panic: &panic,
        drops: &drops,
    });
    let b = a.clone();
    panic.set(true);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            Rc::make_mut(&mut a);
        }))
        .is_err()
    );
    assert!(Rc::ptr_eq(&a, &b));
    assert_eq!(Rc::strong_count(&a), 2);
    let c = a.clone();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            Rc::unwrap_or_clone(c);
        }))
        .is_err()
    );
    assert_eq!(Rc::strong_count(&a), 2);
    assert_eq!(drops.get(), 0);
    panic.set(false);
    Rc::make_mut(&mut a);
    assert_eq!(Rc::strong_count(&a), 1);
    drop(a);
    drop(b);
    assert_eq!(drops.get(), 2);
}

#[test]
fn final_owner_drops_payload_once_when_drop_panics() {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    struct Bomb<'a>(&'a Cell<u32>);
    impl Drop for Bomb<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
            panic!("final drop");
        }
    }
    let drops = Cell::new(0);
    let first = rc::Rc::new(Bomb(&drops));
    let last = first.clone();
    drop(first);
    assert_eq!(drops.get(), 0);
    assert!(catch_unwind(AssertUnwindSafe(|| drop(last))).is_err());
    assert_eq!(drops.get(), 1);
}

#[test]
fn slice_conversion_clone_panic_cleans_initialized_prefix() {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    struct Element<'a> {
        attempts: &'a Cell<usize>,
        drops: &'a Cell<usize>,
    }
    impl Clone for Element<'_> {
        fn clone(&self) -> Self {
            let n = self.attempts.get();
            self.attempts.set(n + 1);
            assert!(n != 2, "third clone");
            Element {
                attempts: self.attempts,
                drops: self.drops,
            }
        }
    }
    impl Drop for Element<'_> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }
    let attempts = Cell::new(0);
    let drops = Cell::new(0);
    let values: Vec<_> = (0..5)
        .map(|_| Element {
            attempts: &attempts,
            drops: &drops,
        })
        .collect();
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _: rc::Rc<[Element<'_>]> = rc::Rc::from(values.as_slice());
        }))
        .is_err()
    );
    assert_eq!(attempts.get(), 3);
    assert_eq!(drops.get(), 2);
    let owner: rc::Rc<[Element<'_>]> = values.into();
    let shared = owner.clone();
    drop(owner);
    assert_eq!(drops.get(), 2);
    drop(shared);
    assert_eq!(drops.get(), 7);
}

fn bounded(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(30)).completed()
}

#[test]
fn rc_ownership_matches_rustc() {
    compare_native("rc_ownership");
}

#[test]
fn recursive_storage_matches_rustc() {
    compare_native("recursive_storage");
}

#[test]
fn closure_residuals_match_rustc() {
    compare_native("closure_residuals");
}

#[test]
fn hir_library_prerequisites_match_rustc() {
    compare_native("hir_library_prerequisites");
}

#[test]
fn hir_additional_apis_match_rustc() {
    compare_native("hir_additional_apis");
}

#[test]
fn tuple_assignment_matches_rustc() {
    compare_native("tuple_assignment");
}

#[test]
fn aggregate_drop_liveness_matches_rustc() {
    compare_native("aggregate_drop_liveness");
}

#[test]
fn recursive_auto_traits_match_rustc() {
    compare_native("recursive_auto_traits");
}

#[test]
fn implicit_argument_reborrows_match_rustc() {
    compare_native("implicit_argument_reborrows");
}

#[test]
fn projected_literal_promotion_matches_rustc() {
    compare_native("projected_literal_promotion");
}

#[test]
fn iterator_item_regions_match_rustc() {
    compare_native("iterator_item_regions");
}

#[test]
fn array_iterator_clone_matches_rustc() {
    compare_native("array_iterator_clone");
}

#[test]
fn slice_single_value_matches_rustc() {
    compare_native("slice_single_value");
}

#[test]
fn mir_library_prerequisites_match_rustc() {
    compare_native("mir_library_prerequisites");
}

#[test]
fn owned_drop_regions_match_rustc() {
    compare_native("owned_drop_regions");
}

#[test]
fn exported_macro_collection_matches_rustc() {
    compare_native("exported_macro_collection");
}

#[test]
fn self_enum_variants_match_rustc() {
    compare_native("self_enum_variants");
}

#[test]
fn forwarded_callable_projections_match_rustc() {
    compare_native("forwarded_callable_projections");
}

#[test]
fn slice_pattern_places_match_rustc() {
    compare_native("slice_pattern_places");
}

#[test]
fn fn_pointer_regions_match_rustc() {
    compare_native("fn_pointer_regions");
}

fn compare_native(case: &str) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/rc-ownership/{}/{}", std::process::id(), case));
    std::fs::create_dir_all(&root).unwrap();
    let source = repo.join(format!("tests/run/{case}.rs"));
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
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
