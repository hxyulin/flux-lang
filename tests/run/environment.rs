fn main() {
    println!("{} {} {}", std::env::consts::OS, std::env::consts::FAMILY, std::env::consts::ARCH);
    // The dedicated harness supplies these values. Ordinary run-suite execution
    // takes the NotPresent branches, which are also compared with rustc.
    for key in ["FLUX_SELFHOST_ENV_TEXT", "FLUX_SELFHOST_ENV_BYTES", "FLUX_SELFHOST_ENV_MISSING", "invalid=key", "nul\0key"] {
        match std::env::var(key) {
            Ok(value) => println!("{key}: {value:?}"),
            Err(error) => {
                println!("{key}: {error}; {error:?}");
                if let std::env::VarError::NotUnicode(value) = error {
                    println!("lossy {:?} {}", value.to_string_lossy(), value.clone().into_string().is_err());
                }
            }
        }
    }
    let os = std::ffi::OsString::from("line\n\0β");
    assert_eq!(os.clone().into_string().unwrap(), "line\n\0β");
    println!("os {os:?} {:?}", os.to_str());
}
