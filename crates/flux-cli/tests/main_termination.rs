//! Program return values use the standard Termination contract.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::process::Command;

#[test]
fn main_results_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-main-results-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "explicit_exit",
            "fn main(){print!(\"partial\");std::process::exit(23)}",
        ),
        (
            "exit_tls",
            "struct End(&'static str);impl Drop for End{fn drop(&mut self){print!(\"{}\",self.0);}}std::thread_local!{static TLS:End=End(\"tls\");}fn main(){let _stack=End(\"stack\");TLS.with(|_|());print!(\"body\");std::process::exit(17)}",
        ),
        (
            "success",
            "fn main()->std::process::ExitCode{std::process::ExitCode::SUCCESS}",
        ),
        (
            "custom_status",
            "fn main()->std::process::ExitCode{std::process::ExitCode::from(37)}",
        ),
        (
            "maximum_status",
            "fn main()->std::process::ExitCode{std::process::ExitCode::from(255)}",
        ),
        (
            "opaque_main",
            "fn main()->impl std::process::Termination{std::process::ExitCode::from(21)}",
        ),
        (
            "nested_opaque_main",
            "fn main()->Result<impl std::process::Termination,&'static str>{Ok(std::process::ExitCode::from(23))}",
        ),
        (
            "result_ok",
            "fn main()->Result<std::process::ExitCode,&'static str>{Ok(std::process::ExitCode::from(9))}",
        ),
        (
            "result_err",
            "fn main()->Result<(),&'static str>{Err(\"failure\")}",
        ),
        (
            "custom_large",
            "struct Done([u64;32]);impl Drop for Done{fn drop(&mut self){println!(\"drop {}\",self.0[31]);}}impl std::process::Termination for Done{fn report(self)->std::process::ExitCode{assert_eq!(self.0[0],7);std::process::ExitCode::from(13)}}fn main()->Done{Done([7;32])}",
        ),
        (
            "api",
            "fn main()->std::process::ExitCode{let code=std::process::ExitCode::default();assert!(code==std::process::ExitCode::SUCCESS);println!(\"{:?}\",std::process::ExitCode::from(7));code}",
        ),
        (
            "alias_std",
            "#![no_std]\nextern crate std as s;fn main()->s::process::ExitCode{s::process::ExitCode::SUCCESS}",
        ),
        ("invalid_integer", "fn main()->i32{7}"),
        ("invalid_custom", "struct Done;fn main()->Done{Done}"),
        (
            "invalid_error",
            "struct Error;fn main()->Result<(),Error>{Err(Error)}",
        ),
        ("invalid_parameter", "fn main(_:u8){}"),
    ] {
        let input = root.join(format!("{name}.rs"));
        std::fs::write(&input, source).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&input)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        for optimized in [false, true] {
            let output = root.join(format!("{name}-{optimized}"));
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_flux"));
            cmd.arg("build").arg(&input).arg("-o").arg(&output);
            if optimized {
                cmd.arg("-O");
            }
            let flux = cmd.output().unwrap();
            assert_eq!(
                flux.status.success(),
                rustc.status.success(),
                "{name} optimized={optimized}: rustc={} flux={}",
                String::from_utf8_lossy(&rustc.stderr),
                String::from_utf8_lossy(&flux.stderr)
            );
            if rustc.status.success() {
                let expected = Command::new(&reference).output().unwrap();
                let actual = Command::new(&output).output().unwrap();
                assert_eq!(actual.status.code(), expected.status.code(), "{name}");
                assert_eq!(actual.stdout, expected.stdout, "{name}");
                assert_eq!(actual.stderr, expected.stderr, "{name}");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
