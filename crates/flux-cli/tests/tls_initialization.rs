//! Host unwinding tests for the unchanged embedded TLS implementation.
mod support;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[test]
fn tls_initializer_unwind_and_cell_ownership_contracts() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = repo.join(format!("out/tls-initialization/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let tls = repo.join("library/std/src/thread").canonicalize().unwrap();
    let source = include_str!("fixtures/tls/initialization.rs.txt")
        .replace("__TLS_DIRECTORY__", &format!("{tls:?}"));
    let input = directory.join("initialization.rs");
    std::fs::write(&input, source).unwrap();
    let executable = directory.join("host-tests");
    for (label, command) in [
        (
            "compile",
            Command::new("rustc")
                .args(["--edition=2024", "--test", "-Dwarnings"])
                .arg(input)
                .arg("-o")
                .arg(&executable),
        ),
        ("run", Command::new(&executable).arg("--test-threads=1")),
    ] {
        let output =
            support::process::run(command, &directory, label, Duration::from_secs(30)).completed();
        support::oracle::success(&output, label);
    }
}
