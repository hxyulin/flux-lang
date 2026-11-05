//! Known trait-solver bugs, each checked against rustc's answer. A test
//! marked `trait solver consolidation` fails today for the reason in its
//! comment; `cargo test -p flux-ty --test trait_solver -- --ignored` lists
//! what is still broken. Remove the marker once the bug is fixed.
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Checks `source` with rustc as a library, from a temporary file.
fn rustc(name: &str, source: &str) -> Output {
    let stem = format!("flux-trait-solver-{}-{name}", std::process::id());
    let path = std::env::temp_dir().join(format!("{stem}.rs"));
    let metadata = path.with_extension("rmeta");
    std::fs::write(&path, source).unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
        .args(["-A", "warnings"])
        .arg(&path)
        .arg("-o")
        .arg(&metadata)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&metadata);
    let _ = std::fs::remove_file(&path);
    output
}

/// flux's type-checking diagnostics for `source`, which must lower cleanly.
fn flux(name: &str, source: &str) -> Vec<String> {
    let (_, _, hir) = flux_hir::lower_text(format!("{name}.rs"), source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_ty::check_file(&hir);
    typed.diagnostics.into_iter().map(|d| d.message).collect()
}

/// rustc reports `code` with a message starting with `message`, and flux
/// reports a diagnostic starting with the same text.
fn rejects(name: &str, source: &str, code: &str, message: &str) {
    let rustc = rustc(name, source);
    let stderr = String::from_utf8_lossy(&rustc.stderr);
    assert!(
        stderr.contains(&format!("error[{code}]: {message}")),
        "rustc {name}: expected {code} `{message}`, got:\n{stderr}"
    );
    let found = flux(name, source);
    assert!(
        found.iter().any(|m| m.starts_with(message)),
        "flux {name}: expected `{message}` ({code}), got {found:?}"
    );
}

/// rustc and flux both accept `source`.
fn accepts(name: &str, source: &str) {
    let rustc = rustc(name, source);
    assert!(
        rustc.status.success(),
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let found = flux(name, source);
    assert!(found.is_empty(), "flux {name}: {found:?}");
}

// Coherence treats a bound that fails for the other impl's parameters as
// proof that two impls cannot overlap, and counts foreign traits and
// types as negative evidence. flux accepts both.

#[test]
#[ignore = "trait solver consolidation"]
fn overlapping_blanket_impls() {
    rejects(
        "overlapping_blanket_impls",
        "trait Tr {}\nimpl<T: Copy> Tr for T {}\nimpl<T: Clone> Tr for T {}",
        "E0119",
        "conflicting implementations of trait `Tr`",
    );
}

#[test]
#[ignore = "trait solver consolidation"]
fn blanket_impl_overlaps_foreign_type() {
    rejects(
        "blanket_impl_overlaps_foreign_type",
        "trait Tr2 {}\nimpl<T: core::fmt::Display> Tr2 for T {}\nimpl Tr2 for Vec<u8> {}",
        "E0119",
        "conflicting implementations of trait `Tr2`",
    );
}

// Outside bodies, every `Sized` goal on a parameter holds, even when the
// parameter is `?Sized`. flux accepts all three.

#[test]
#[ignore = "trait solver consolidation"]
fn impl_method_adds_sized_bound() {
    rejects(
        "impl_method_adds_sized_bound",
        "trait Tr { fn f<T: ?Sized>(x: &T) -> usize; }\n\
         impl Tr for u8 { fn f<T: ?Sized>(_: &T) -> usize where T: Sized { 0 } }",
        "E0276",
        "impl has stricter requirements than trait",
    );
}

#[test]
#[ignore = "trait solver consolidation"]
fn unsized_parameter_in_signature_bound() {
    rejects(
        "unsized_parameter_in_signature_bound",
        "trait Tr {}\nimpl<T> Tr for Box<T> {}\nstruct Need<X: Tr>(X);\n\
         fn g<U: ?Sized>(_: &Need<Box<U>>) {}",
        "E0277",
        "the size for values of type `U` cannot be known at compilation time",
    );
}

#[test]
#[ignore = "trait solver consolidation"]
fn drop_impl_requires_sized() {
    rejects(
        "drop_impl_requires_sized",
        "struct S<T: ?Sized>(Box<T>);\nimpl<T> Drop for S<T> { fn drop(&mut self) {} }",
        "E0367",
        "`Drop` impl requires `T: Sized` but the struct it is implemented for does not",
    );
}

// `u8: Tr` needs `W<u8>: Tr`, which needs `W<W<u8>>: Tr`, without end.
// Bodies report the overflow; signatures and impl headers stop at a depth
// limit and report "the trait bound ... is not satisfied" (E0277).

const RECURSIVE: &str = "trait Tr { fn m(&self) -> u8 { 1 } }\nstruct W<T>(T);\n\
                         impl<T> Tr for T where W<T>: Tr {}\n";

#[test]
fn recursive_impl_in_body() {
    rejects(
        "recursive_impl_in_body",
        &format!("{RECURSIVE}fn need<T: Tr>() {{}}\npub fn f() {{ need::<u8>(); }}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
fn recursive_impl_in_method_call() {
    rejects(
        "recursive_impl_in_method_call",
        &format!("{RECURSIVE}pub fn f() -> u8 {{ 0u8.m() }}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
#[ignore = "trait solver consolidation"]
fn recursive_impl_in_signature() {
    rejects(
        "recursive_impl_in_signature",
        &format!("{RECURSIVE}struct Need<X: Tr>(X);\npub fn g(_: Need<u8>) {{}}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
#[ignore = "trait solver consolidation"]
fn recursive_impl_in_impl_where_clause() {
    rejects(
        "recursive_impl_in_impl_where_clause",
        &format!("{RECURSIVE}trait M {{}}\nimpl M for u8 where u8: Tr {{}}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

/// More nested adapters than the 32 goals at which `select_impl` stops.
/// Type checking accepts it; `tests/run/pending` has the whole program.
#[test]
fn deep_iterator_chain() {
    accepts(
        "deep_iterator_chain",
        include_str!("../../../tests/run/pending/deep_iterator_chain.rs"),
    );
}

/// `struct A0(A1, A1); ... struct A29(A30, A30);` is a DAG of 31 types.
/// `holds` explored it as a tree, so the `Sync` check of the static made
/// about 2^30 calls: 2.6 s at depth 22 and 9 s at depth 24 in a release
/// build. It proves each nested goal once now.
#[test]
fn shared_type_sync_check_is_bounded() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "sync_probe", "--nocapture"])
        .env("FLUX_SYNC_DEPTH", "30")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let limit = Duration::from_secs(10);
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "sync probe: {status}");
            return;
        }
        if start.elapsed() > limit {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the Sync check of a depth-30 shared type took more than {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "subprocess entry point; run through the bounded parent"]
fn sync_probe() {
    let Ok(depth) = std::env::var("FLUX_SYNC_DEPTH") else {
        return;
    };
    let depth: usize = depth.parse().unwrap();
    let mut source = format!("struct A{depth};\n");
    for i in (0..depth).rev() {
        source.push_str(&format!("struct A{i}(A{}, A{});\n", i + 1, i + 1));
    }
    source.push_str("static S: Option<A0> = None;");
    let start = Instant::now();
    accepts("shared_type_sync_check", &source);
    println!("sync check at depth {depth}: {:?}", start.elapsed());
}
