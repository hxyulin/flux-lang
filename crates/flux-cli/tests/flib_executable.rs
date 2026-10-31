//! A staged identity detects rebuilds while allowing unchanged copied binaries.
#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use flux_cli::flib::executable::{CheckMethod, ExecutableIdentity};
use flux_hir::flib::{Decode, Decoder, Encode, Encoder};

#[test]
fn stat_hash_and_copy_paths_preserve_content_identity() {
    let s = scratch::Scratch::new(std::env::temp_dir().join("flux-executable-identity"));
    let file = s.path().join("compiler");
    std::fs::write(&file, b"original executable").unwrap();
    let identity = ExecutableIdentity::capture(&file).unwrap();
    let mut e = Encoder::new();
    identity.encode(&mut e).unwrap();
    let bytes = e.into_bytes();
    let mut d = Decoder::new(&bytes);
    let identity = ExecutableIdentity::decode(&mut d).unwrap();
    d.finish().unwrap();
    let checked = identity.check(&file).unwrap();
    assert_eq!(checked.hash, identity.hash());
    assert_eq!(checked.method, CheckMethod::Stat);
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    let checked = identity.check(&file).unwrap();
    assert_eq!(checked.hash, identity.hash());
    assert_eq!(checked.method, CheckMethod::Hash);
    let copy = s.path().join("installed-compiler");
    std::fs::copy(&file, &copy).unwrap();
    let checked = identity.check(&copy).unwrap();
    assert_eq!(checked.method, CheckMethod::Hash);
    assert_eq!(checked.hash, identity.hash());
    // A linker replaces the executable, including for a dirty source rebuild.
    let replacement = s.path().join("replacement");
    std::fs::write(&replacement, b"modified executable").unwrap();
    std::fs::rename(replacement, &file).unwrap();
    let checked = identity.check(&file).unwrap();
    assert_eq!(checked.method, CheckMethod::Hash);
    assert_ne!(checked.hash, identity.hash());
    std::fs::write(&file, b"different executable length").unwrap();
    assert_ne!(identity.check(&file).unwrap().hash, identity.hash());
}

#[test]
fn report_real_executable_check_costs() {
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_fluxc"));
    let identity = ExecutableIdentity::capture(executable).unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        identity.check(executable).unwrap().method,
        CheckMethod::Stat
    );
    eprintln!("executable identity stat fast path: {:?}", start.elapsed());
    let s = scratch::Scratch::new(std::env::temp_dir().join("flux-executable-copy"));
    let copy = s.path().join("compiler");
    std::fs::copy(executable, &copy).unwrap();
    let start = std::time::Instant::now();
    let checked = identity.check(&copy).unwrap();
    assert_eq!(checked.method, CheckMethod::Hash);
    assert_eq!(checked.hash, identity.hash());
    eprintln!("executable identity hash fallback: {:?}", start.elapsed());
}
