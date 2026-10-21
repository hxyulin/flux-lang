//! Compiles every program in `tests/run` with rustc and with flux
//! (unoptimized and with `-O`), runs each, and compares their output and
//! exit status. rustc gets `-C overflow-checks=off -C panic=abort`, the
//! semantics flux implements.
//!
//! Executables can only be linked for arm64 macOS so far; elsewhere the
//! test reports that it was skipped.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    stdout: String,
    /// The exit code, 128 plus the signal that killed the program, or -1
    /// if it ran past the time limit.
    status: i32,
}

impl Outcome {
    /// For failure messages: the status and the start of the output.
    fn brief(&self) -> String {
        let mut stdout: String = self.stdout.chars().take(200).collect();
        if stdout.len() < self.stdout.len() {
            stdout.push_str("...");
        }
        format!("status {} with output {stdout:?}", self.status)
    }
}

/// How long a program may run before it counts as hung.
const TIME_LIMIT: Duration = Duration::from_secs(10);

fn run(executable: &Path) -> Outcome {
    // Panic messages go to stderr, which is not compared.
    let mut child = Command::new(executable)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", executable.display()));
    let mut stdout = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if start.elapsed() > TIME_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = String::from_utf8_lossy(&reader.join().unwrap()).into_owned();
    let status = match status {
        None => -1,
        Some(status) => status.code().unwrap_or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            128 + status.signal().unwrap_or(0)
        }),
    };
    Outcome { stdout, status }
}

fn compile(command: &mut Command, what: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|e| format!("cannot start {what}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{what} failed:\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[test]
fn programs_match_rustc() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        eprintln!("skipped: flux links executables only on arm64 macOS so far");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(root.join("tests/run"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty());
    let dir = std::env::temp_dir().join(format!("flux-run-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let mut failures = Vec::new();
    for program in &programs {
        let name = program.file_stem().unwrap().to_string_lossy().into_owned();
        let reference = dir.join(format!("{name}-rustc"));
        let result = compile(
            Command::new("rustc")
                .args([
                    "--edition",
                    "2024",
                    "-C",
                    "overflow-checks=off",
                    "-C",
                    "panic=abort",
                ])
                .arg("-o")
                .arg(&reference)
                .arg(program),
            "rustc",
        );
        if let Err(message) = result {
            failures.push(format!("{name}: {message}"));
            continue;
        }
        let expected = run(&reference);
        for optimize in [false, true] {
            let suffix = if optimize { "-O" } else { "" };
            let executable = dir.join(format!("{name}-flux{suffix}"));
            let mut flux = Command::new(env!("CARGO_BIN_EXE_flux"));
            flux.arg("build").arg(program).arg("-o").arg(&executable);
            if optimize {
                flux.arg("-O");
            }
            if let Err(message) = compile(&mut flux, "flux") {
                failures.push(format!("{name}{suffix}: {message}"));
                continue;
            }
            let actual = run(&executable);
            if actual != expected {
                failures.push(format!(
                    "{name}{suffix}: expected {}, got {}",
                    expected.brief(),
                    actual.brief()
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
