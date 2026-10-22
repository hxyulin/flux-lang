//! Compiles every program in `tests/run` with rustc and with flux
//! (unoptimized and with `-O`), runs each, and compares their output,
//! panic messages and exit status. rustc gets `-C overflow-checks=off
//! -C panic=abort`, the semantics flux implements. Both compilers are
//! given the program's path relative to the repository, which panic
//! messages show; the thread id they show differs between runs.
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
    /// With the thread id in panic messages replaced by `N`.
    stderr: String,
    /// The exit code, 128 plus the signal that killed the program, or -1
    /// if it ran past the time limit.
    status: i32,
}

impl Outcome {
    /// For failure messages: the status and the start of the output.
    fn brief(&self) -> String {
        let cut = |s: &str| {
            let mut out: String = s.chars().take(300).collect();
            if out.len() < s.len() {
                out.push_str("...");
            }
            out
        };
        format!(
            "status {} with output {:?} and stderr {:?}",
            self.status,
            cut(&self.stdout),
            cut(&self.stderr)
        )
    }
}

/// How long a program may run before it counts as hung.
const TIME_LIMIT: Duration = Duration::from_secs(10);

/// `stderr` with the thread id of `thread 'main' (123) panicked`
/// replaced by `N`.
fn normalize(stderr: &str) -> String {
    let marker = "thread 'main' (";
    let mut out = String::new();
    let mut rest = stderr;
    while let Some(i) = rest.find(marker) {
        let after = &rest[i + marker.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        out.push_str(&rest[..i + marker.len()]);
        if digits > 0 && after[digits..].starts_with(')') {
            out.push('N');
            rest = &after[digits..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn run(executable: &Path) -> Outcome {
    let mut child = Command::new(executable)
        .env_remove("RUST_BACKTRACE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot run {}: {e}", executable.display()));
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
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
    let stderr = normalize(&String::from_utf8_lossy(&err_reader.join().unwrap()));
    let status = match status {
        None => -1,
        Some(status) => status.code().unwrap_or_else(|| {
            use std::os::unix::process::ExitStatusExt;
            128 + status.signal().unwrap_or(0)
        }),
    };
    Outcome {
        stdout,
        stderr,
        status,
    }
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let mut programs: Vec<PathBuf> = std::fs::read_dir(root.join("tests/run"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty());
    let dir = std::env::temp_dir().join(format!("flux-run-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    compare_programs(&root, &dir, &programs);
}

/// A small executable milestone toward compiling flux with itself.
#[test]
fn compiler_modules_match_rustc() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        eprintln!("skipped: flux links executables only on arm64 macOS so far");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("flux-compiler-tests-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Compile unchanged compiler implementations, rather than copies of
    // their algorithms. Bundle modules until flux supports `#[path]`.
    let internals = dir.join("compiler_internals.rs");
    std::fs::write(
        &internals,
        format!(
            "mod syntax_kind {{\n{}\n{}\n}}\npub use syntax_kind::SyntaxKind;\nmod rng {{\n{}\n}}\nmod bitset {{\n{}\n}}\nmod interval {{\n{}\n}}\nmod event {{\n{}\n{}\n}}\nmod generator_types {{\n{}\n{}\n}}\nmod tree {{\n{}\n{}\n}}\n{}",
            include_str!("../../flux-syntax/src/syntax_kind.rs"),
            include_str!("../../../tests/run/support/compiler_syntax_kind.rs"),
            include_str!("../../flux-difftest/src/rng.rs"),
            include_str!("../../flux-mir/src/bitset.rs"),
            include_str!("../../flux-mir/src/interval.rs"),
            include_str!("../../flux-syntax/src/parser/event.rs"),
            include_str!("../../../tests/run/support/compiler_events.rs"),
            include_str!("../../flux-difftest/src/generator/ty.rs"),
            include_str!("../../../tests/run/support/compiler_generator_types.rs"),
            include_str!("../../flux-syntax/src/tree.rs"),
            include_str!("../../../tests/run/support/compiler_tree.rs"),
            include_str!("../../../tests/run/support/compiler_internals.rs"),
        ),
    )
    .unwrap();
    compare_programs(&root, &dir, &[internals]);
}

fn compare_programs(root: &Path, dir: &Path, programs: &[PathBuf]) {
    let mut failures = Vec::new();
    for program in programs {
        let name = program.file_stem().unwrap().to_string_lossy().into_owned();
        // As given on the command line, which panic messages show.
        let program = program.strip_prefix(root).unwrap_or(program);
        let reference = dir.join(format!("{name}-rustc"));
        let result = compile(
            Command::new("rustc")
                .current_dir(root)
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
            flux.current_dir(root)
                .arg("build")
                .arg(program)
                .arg("-o")
                .arg(&executable);
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
    let _ = std::fs::remove_dir_all(dir);
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}
