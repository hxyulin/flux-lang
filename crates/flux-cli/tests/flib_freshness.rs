mod flib_support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use flux_cli::flib::freshness::Snapshot;
use flux_hir::{CrateId, ItemTree, SourceDb};
use flux_intern::Symbol;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "flux-flib-freshness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn lower(path: &Path, environment: Option<&BTreeMap<String, String>>) -> (ItemTree, CrateId) {
    let mut db = SourceDb::new();
    let root = db.load(path).unwrap();
    if let Some(environment) = environment {
        db.set_crate_env(root, environment.clone()).unwrap();
    }
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("freshness"));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let krate = hir.items.main_crate();
    (hir.items, krate)
}
fn capture(path: &Path) -> Snapshot {
    let (items, krate) = lower(path, None);
    Snapshot::capture(&items, krate, b"config", &[[1; 32]])
        .unwrap()
        .unwrap()
}

#[test]
fn unchanged_inputs_round_trip_and_configuration_changes_invalidate() {
    let dir = Scratch::new();
    let root = dir.write("lib.rs", "pub fn value() -> u32 { 1 }\n");
    let snapshot = capture(&root);
    assert!(snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    assert!(!snapshot.is_fresh(b"changed", &[[1; 32]], None).unwrap());
    assert!(!snapshot.is_fresh(b"config", &[[2; 32]], None).unwrap());
    let bytes = snapshot.to_bytes().unwrap();
    let copy = Snapshot::from_bytes(&bytes).unwrap();
    assert_eq!(snapshot, copy);
    assert_eq!(bytes, copy.to_bytes().unwrap());
    for n in 0..bytes.len() {
        assert!(Snapshot::from_bytes(&bytes[..n]).is_err(), "prefix {n}");
    }
    let mut wrong_compiler = bytes;
    wrong_compiler[12] ^= 1;
    assert!(
        !Snapshot::from_bytes(&wrong_compiler)
            .unwrap()
            .is_fresh(b"config", &[[1; 32]], None)
            .unwrap()
    );
}

#[test]
fn same_size_edits_with_original_mtime_still_invalidate() {
    let dir = Scratch::new();
    let root = dir.write("lib.rs", "pub fn value() -> u32 { 1 }\n");
    let modified = std::fs::metadata(&root).unwrap().modified().unwrap();
    let snapshot = capture(&root);
    std::fs::write(&root, "pub fn value() -> u32 { 2 }\n").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&root)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
}

#[test]
fn loaded_modules_and_new_alternate_candidates_invalidate() {
    let dir = Scratch::new();
    let root = dir.write(
        "lib.rs",
        "mod child; pub fn value() -> u32 { child::value() }\n",
    );
    let child = dir.write("child/mod.rs", "pub fn value() -> u32 { 1 }\n");
    let snapshot = capture(&root);
    assert!(snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    let alternate = dir.write("child.rs", "pub fn value() -> u32 { 2 }\n");
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    std::fs::remove_file(alternate).unwrap();
    assert!(snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    std::fs::remove_file(child).unwrap();
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
}

#[test]
fn include_bytes_and_include_str_inputs_are_content_tracked() {
    for (macro_name, literal) in [
        (
            "include_str",
            "pub const DATA: &str = include_str!(\"data\");\n",
        ),
        (
            "include_bytes",
            "pub const DATA: &[u8] = include_bytes!(\"data\");\n",
        ),
    ] {
        let dir = Scratch::new();
        let root = dir.write("lib.rs", literal);
        let data = dir.write("data", b"first");
        let snapshot = capture(&root);
        assert!(
            snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap(),
            "{macro_name}"
        );
        std::fs::write(data, b"other").unwrap();
        assert!(
            !snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap(),
            "{macro_name}"
        );
    }
}

#[test]
fn source_include_inputs_are_content_tracked() {
    let dir = Scratch::new();
    let root = dir.write("lib.rs", "include!(\"included.rs\");\n");
    let included = dir.write("included.rs", "pub const N: u32 = 1;\n");
    let snapshot = capture(&root);
    assert!(snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    std::fs::write(included, "pub const N: u32 = 2;\n").unwrap();
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
}

#[test]
fn explicit_environment_values_and_absence_are_part_of_snapshot() {
    let dir = Scratch::new();
    let root=dir.write("lib.rs","pub const VALUE: &str = env!(\"FLUX_PRESENT\"); pub const MAYBE: Option<&str> = option_env!(\"FLUX_ABSENT\");\n");
    let mut environment = BTreeMap::new();
    environment.insert("FLUX_PRESENT".into(), "one".into());
    let (items, krate) = lower(&root, Some(&environment));
    let snapshot = Snapshot::capture(&items, krate, b"config", &[])
        .unwrap()
        .unwrap();
    assert!(
        snapshot
            .is_fresh(b"config", &[], Some(&environment))
            .unwrap()
    );
    environment.insert("FLUX_ABSENT".into(), "now present".into());
    assert!(
        !snapshot
            .is_fresh(b"config", &[], Some(&environment))
            .unwrap()
    );
    environment.remove("FLUX_ABSENT");
    environment.insert("FLUX_PRESENT".into(), "two".into());
    assert!(
        !snapshot
            .is_fresh(b"config", &[], Some(&environment))
            .unwrap()
    );
}

#[cfg(unix)]
#[test]
fn symlink_retargeting_invalidates_even_equal_file_contents() {
    let dir = Scratch::new();
    let first = dir.write("first.rs", "pub fn value() -> u32 { 1 }\n");
    let second = dir.write("second.rs", "pub fn value() -> u32 { 1 }\n");
    let root = dir.0.join("lib.rs");
    std::os::unix::fs::symlink(first, &root).unwrap();
    let snapshot = capture(&root);
    std::fs::remove_file(&root).unwrap();
    std::os::unix::fs::symlink(second, &root).unwrap();
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
}

#[test]
fn generated_source_at_a_different_disk_content_is_not_reusable() {
    let dir = Scratch::new();
    let root = dir.write("lib.rs", "pub fn old() {}\n");
    let mut db = SourceDb::new();
    let id = db.add(&root, "pub fn generated() {}\n");
    let hir = flux_hir::lower(&mut db, id, Symbol::intern("generated"));
    assert!(
        Snapshot::capture(&hir.items, hir.items.main_crate(), b"config", &[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn process_environment_change_is_stale() {
    const CHILD: &str = "FLUX_FLIB_FRESHNESS_CHILD_SNAPSHOT";
    const INPUT: &str = "FLUX_FLIB_FRESHNESS_PROCESS_INPUT";
    if let Some(path) = std::env::var_os(CHILD) {
        let snapshot = Snapshot::from_bytes(&std::fs::read(path).unwrap()).unwrap();
        assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
        return;
    }
    let dir = Scratch::new();
    let root = dir.write(
        "lib.rs",
        format!("pub const VALUE: Option<&str> = option_env!(\"{INPUT}\");\n"),
    );
    let snapshot = capture(&root);
    let path = dir.write("snapshot.bin", snapshot.to_bytes().unwrap());
    let previous = std::env::var(INPUT).ok();
    let next = if previous.as_deref() == Some("changed") {
        "different"
    } else {
        "changed"
    };
    let result = flib_support::run(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_environment_change_is_stale"])
            .env(CHILD, path)
            .env(INPUT, next),
        "environment",
    );
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn indirect_include_macro_inputs_are_tracked() {
    let dir = Scratch::new();
    let root=dir.write("lib.rs","macro_rules! read_text { () => { include_str!(\"data\") }; } pub const DATA: &str = read_text!();\n");
    let data = dir.write("data", b"first");
    let snapshot = capture(&root);
    assert!(snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
    std::fs::write(data, b"other").unwrap();
    assert!(!snapshot.is_fresh(b"config", &[[1; 32]], None).unwrap());
}

#[test]
fn module_candidate_created_during_compilation_declines_reuse() {
    let dir = Scratch::new();
    let root = dir.write("lib.rs", "mod child;\n");
    dir.write("child/mod.rs", "pub fn value() {}\n");
    let (items, krate) = lower(&root, None);
    dir.write("child.rs", "pub fn other() {}\n");
    assert!(
        Snapshot::capture(&items, krate, b"config", &[])
            .unwrap()
            .is_none()
    );
}
