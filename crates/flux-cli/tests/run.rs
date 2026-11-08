//! Compiles every program in `tests/run` with rustc and with flux
//! (unoptimized and with `-O`), runs each, and compares their output,
//! panic messages and exit status. rustc gets `-C overflow-checks=off
//! -C panic=abort`, the semantics flux implements. Debug assertions are
//! on without `-O` and off with it in both compilers; a program that
//! mentions them (`debug_assert*!`, `cfg(debug_assertions)`) gets a second
//! rustc reference built with `-C debug-assertions=off`, while other
//! programs cannot observe the setting and share one. Both compilers are
//! given the program's path relative to the repository, which panic
//! messages show; the thread id they show differs between runs.
//!
//! Executables can only be linked for arm64 macOS so far; elsewhere the
//! test reports that it was skipped.

#[path = "../../../tests/support/cases.rs"]
mod cases;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use scratch::Scratch;

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

/// This fixture checks every Unicode scalar, rather than a small sample.
/// Both baseline Flux modes take about 17 seconds on the validation host,
/// so it gets a longer limit and starts first instead of running alone at
/// the end.
const SLOW_PROGRAM: &str = "unicode_properties";

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

fn run(executable: &Path, time_limit: Duration) -> Outcome {
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
        if start.elapsed() > time_limit {
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
    programs.sort_by_key(|path| path.file_stem() != Some(SLOW_PROGRAM.as_ref()));
    assert!(!programs.is_empty());
    programs.retain(|path| cases::path_selected(path));
    if programs.is_empty() {
        return;
    }
    let scratch = Scratch::new(std::env::temp_dir().join("flux-run-tests"));
    compare_programs(&root, scratch.path(), &programs);
}

/// A small executable milestone toward compiling flux with itself.
#[test]
fn compiler_modules_match_rustc() {
    if !cases::selected("compiler_modules") {
        return;
    }
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        eprintln!("skipped: flux links executables only on arm64 macOS so far");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch::new(std::env::temp_dir().join("flux-compiler-tests"));
    let dir = scratch.path();

    // Compile unchanged compiler implementations, rather than copies of
    // their algorithms. Bundle modules until flux supports `#[path]`.
    let internals = dir.join("compiler_internals.rs");
    let syntax = include_str!("../../flux-syntax/src/lib.rs");
    let edition_start = syntax
        .find("/// Rust edition of one source crate.")
        .unwrap();
    let edition_end = syntax.find("/// A problem found while lexing").unwrap();
    let edition = &syntax[edition_start..edition_end];
    std::fs::write(
        &internals,
        format!(
            "{edition}\nmod syntax_kind {{\n{}\n{}\n}}\npub use syntax_kind::SyntaxKind;\nmod rng {{\n{}\n}}\nmod bitset {{\n{}\n}}\nmod interval {{\n{}\n}}\nmod event {{\n{}\n{}\n}}\nmod generator_types {{\n{}\n{}\n}}\nmod tree {{\n{}\n{}\n}}\n{}",
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
    compare_programs(&root, dir, &[internals]);
}

/// `-C debug-assertions` overrides the default that `-O` selects.
#[test]
fn debug_assertions_flag_overrides_optimization_default() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64"))
        || !cases::selected("debug_assertions_flag")
    {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = Scratch::new(std::env::temp_dir().join("flux-debug-assertions-flag"));
    for (flags, expected) in [
        (
            &["-O", "-C", "debug-assertions"][..],
            "debug assertions true true\n",
        ),
        (
            &["--fast-build", "-C", "debug-assertions"][..],
            "debug assertions true true\n",
        ),
        (
            &["-Cdebug-assertions=no"][..],
            "no debug assertions false false\n",
        ),
        (
            &["-C", "debug-assertions=off", "-C", "debug-assertions=yes"][..],
            "debug assertions true true\n",
        ),
    ] {
        let executable = scratch.path().join("program");
        let mut flux = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        flux.current_dir(&root)
            .arg("build")
            .arg("tests/run/debug_assertions.rs")
            .args(flags)
            .arg("-o")
            .arg(&executable);
        compile(&mut flux, "flux").unwrap();
        let outcome = run(&executable, TIME_LIMIT);
        assert!(
            outcome.stdout.starts_with(expected),
            "{flags:?}: {}",
            outcome.brief()
        );
    }
    let output = Command::new(env!("CARGO_BIN_EXE_fluxc"))
        .args([
            "build",
            "-C",
            "opt-level=3",
            "tests/run/debug_assertions.rs",
        ])
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unsupported codegen option `opt-level`")
    );
}

#[test]
fn trait_required_lifetime_matches_rustc() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // The broad sweep owns this ordinary fixture in a complete runner plan.
    if std::env::var_os("FLUX_TEST_NATIVE_SWEEP").is_none()
        && cases::selected("trait_required_lifetime")
    {
        let scratch = Scratch::new(std::env::temp_dir().join("flux-trait-required-lifetime"));
        compare_programs(
            &root,
            scratch.path(),
            &[root.join("tests/run/trait_required_lifetime.rs")],
        );
    }
}

fn compare_programs(root: &Path, dir: &Path, programs: &[PathBuf]) {
    // Keep compiler processes bounded while avoiding a serial build of every fixture.
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(cases::workers())
        .min(programs.len().max(1));
    // Workers take the next program from one queue, so a slow program does
    // not hold back the programs that a fixed chunk would have queued after it.
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(program) = programs.get(index) else {
                        break;
                    };
                    let failures = compare_program(root, dir, program);
                    results.lock().unwrap().push((index, failures));
                }
            });
        }
    });
    // Report failures in program order.
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|&(index, _)| index);
    let failures: Vec<String> = results.into_iter().flat_map(|(_, f)| f).collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

fn compare_program(root: &Path, dir: &Path, program: &Path) -> Vec<String> {
    let mut failures = Vec::new();
    let name = program.file_stem().unwrap().to_string_lossy().into_owned();
    let time_limit = if name == SLOW_PROGRAM {
        Duration::from_secs(60)
    } else {
        TIME_LIMIT
    };
    // As given on the command line, which panic messages show.
    let program = program.strip_prefix(root).unwrap_or(program);
    let observes_assertions =
        std::fs::read_to_string(root.join(program)).is_ok_and(|text| text.contains("debug_assert"));
    let reference = |debug_assertions: bool| {
        let suffix = if debug_assertions { "" } else { "-release" };
        let reference = dir.join(format!("{name}-rustc{suffix}"));
        compile(
            Command::new("rustc")
                .current_dir(root)
                .args([
                    "--edition",
                    "2024",
                    "-C",
                    "overflow-checks=off",
                    "-C",
                    "panic=abort",
                    "-C",
                ])
                .arg(format!(
                    "debug-assertions={}",
                    if debug_assertions { "on" } else { "off" }
                ))
                .arg("-o")
                .arg(&reference)
                .arg(program),
            "rustc",
        )
        .map(|()| run(&reference, time_limit))
    };
    let debug = match reference(true) {
        Ok(outcome) => outcome,
        Err(message) => {
            failures.push(format!("{name}: {message}"));
            return failures;
        }
    };
    let release = if observes_assertions {
        match reference(false) {
            Ok(outcome) => Some(outcome),
            Err(message) => {
                failures.push(format!("{name}: {message}"));
                return failures;
            }
        }
    } else {
        None
    };
    for suffix in ["", "-O", "--fast-build"] {
        let optimize = !suffix.is_empty();
        let expected = match &release {
            Some(release) if optimize => release,
            _ => &debug,
        };
        let executable = dir.join(format!("{name}-flux{suffix}"));
        let mut flux = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        flux.current_dir(root)
            .arg("build")
            .arg(program)
            .arg("-o")
            .arg(&executable);
        if optimize {
            flux.arg(suffix);
        }
        if let Err(message) = compile(&mut flux, "flux") {
            failures.push(format!("{name}{suffix}: {message}"));
            continue;
        }
        let actual = run(&executable, time_limit);
        if actual != *expected {
            failures.push(format!(
                "{name}{suffix}: expected {}, got {}",
                expected.brief(),
                actual.brief()
            ));
        }
    }
    failures
}
