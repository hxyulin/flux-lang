use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};

fn main() {
    let key = String::from("FLUX_ENVIRONMENT_MUTATION_REGRESSION");
    // SAFETY: this program has one thread and no concurrent environment access.
    unsafe { std::env::set_var(&key, "first"); }
    assert_eq!(std::env::var(key.clone()).unwrap(), "first");
    // SAFETY: this program has one thread and no concurrent environment access.
    unsafe { std::env::set_var(&key, OsString::from_vec(vec![b'x', 0xff])); }
    assert_eq!(std::env::var_os(&key).unwrap().as_bytes(), &[b'x', 0xff]);
    assert!(matches!(std::env::var(&key), Err(std::env::VarError::NotUnicode(_))));
    // SAFETY: this program has one thread and no concurrent environment access.
    unsafe { std::env::remove_var(&key); }
    assert!(std::env::var(key).is_err());
    println!("environment mutation passed");
}
