//! Compiling a program with rustc and with flux, running both, and
//! classifying what happened.
//!
//! rustc is the oracle: it compiles with `--edition 2024 -C
//! overflow-checks=off -C panic=abort`, the semantics flux implements.
//! flux compiles twice, without and with `-O`. Every executable runs
//! under `gtimeout`; its stdout and exit status are compared (stderr,
//! where panic messages go, is not).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where the compilers are, and how long things may take.
#[derive(Clone, Debug)]
pub struct Tools {
    pub flux: PathBuf,
    pub rustc: String,
    /// Seconds a compiled program may run.
    pub run_timeout: u32,
    /// Seconds a compiler may take.
    pub compile_timeout: u32,
}

impl Tools {
    /// The flux binary next to this one (both are built in the same
    /// target directory), or `$FLUX`.
    pub fn find() -> Result<Tools, String> {
        let flux = match std::env::var_os("FLUX") {
            Some(p) => PathBuf::from(p),
            None => {
                let me = std::env::current_exe().map_err(|e| e.to_string())?;
                me.with_file_name("flux")
            }
        };
        if !flux.exists() {
            return Err(format!(
                "no flux binary at {} (build it with `cargo build --release -p flux-cli`, or set FLUX)",
                flux.display()
            ));
        }
        Ok(Tools {
            flux,
            rustc: std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned()),
            run_timeout: 10,
            compile_timeout: 60,
        })
    }
}

/// flux without or with `-O`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Debug,
    Opt,
}

impl Mode {
    pub const BOTH: [Mode; 2] = [Mode::Debug, Mode::Opt];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Debug => "debug",
            Mode::Opt => "opt",
        }
    }
}

/// What a program printed and how it exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub stdout: String,
    /// With the thread id of `thread 'main' (123) panicked` replaced by
    /// `N`, as it differs between runs.
    pub stderr: String,
    /// The exit status, 128 + the signal if one killed it, or -1 if it
    /// ran out of time.
    pub status: i32,
}

/// How one flux build of a program went.
#[derive(Clone, Debug)]
pub enum FluxOutcome {
    /// Built, ran and matched rustc's program.
    Same,
    /// Built and ran, differently.
    Differs(Run),
    /// Reported errors in a program rustc accepts.
    Rejected(String),
    /// The compiler panicked or was killed.
    Crashed(String),
    /// The compiler failed past type checking: keel, the linker, or an
    /// internal error.
    Backend(String),
    /// The compiler ran out of time.
    CompileTimeout,
}

/// Everything about one program.
#[derive(Clone, Debug)]
pub enum Verdict {
    /// rustc rejected it: a generator bug.
    InvalidProgram(String),
    /// rustc's program did not finish: a generator bug.
    ReferenceTimeout,
    Tested {
        expected: Run,
        flux: Vec<(Mode, FluxOutcome)>,
    },
}

impl Verdict {
    /// A short class and a signature that identifies the failure for
    /// deduplication and reduction (`None` when everything matched).
    pub fn signature(&self) -> Option<Signature> {
        match self {
            Verdict::InvalidProgram(m) => Some(Signature::new("invalid-program", &first_error(m))),
            Verdict::ReferenceTimeout => Some(Signature::new(
                "invalid-program",
                "rustc's program timed out",
            )),
            Verdict::Tested { expected, flux } => {
                // The most severe failure wins: a crash, a backend error,
                // a rejection, then a miscompilation.
                let failing: Vec<&(Mode, FluxOutcome)> = flux
                    .iter()
                    .filter(|(_, o)| !matches!(o, FluxOutcome::Same))
                    .collect();
                if failing.is_empty() {
                    return None;
                }
                let modes = modes_name(&failing.iter().map(|(m, _)| *m).collect::<Vec<_>>());
                let pick = |f: fn(&FluxOutcome) -> Option<String>| {
                    failing.iter().find_map(|(m, o)| f(o).map(|s| (*m, s)))
                };
                if let Some((_, m)) = pick(|o| match o {
                    FluxOutcome::Crashed(m) => Some(m.clone()),
                    _ => None,
                }) {
                    return Some(Signature::new("crash", &normalize(&panic_site(&m))));
                }
                if let Some((_, m)) = pick(|o| match o {
                    FluxOutcome::Backend(m) => Some(m.clone()),
                    _ => None,
                }) {
                    return Some(Signature::new("backend", &normalize(&first_line(&m))));
                }
                if failing
                    .iter()
                    .any(|(_, o)| matches!(o, FluxOutcome::CompileTimeout))
                {
                    return Some(Signature::new("compile-timeout", &modes));
                }
                if let Some((_, m)) = pick(|o| match o {
                    FluxOutcome::Rejected(m) => Some(m.clone()),
                    _ => None,
                }) {
                    return Some(Signature::new("rejected", &normalize(&first_error(&m))));
                }
                // Miscompilations: which modes, and how.
                let (_, run) = failing
                    .iter()
                    .find_map(|(m, o)| match o {
                        FluxOutcome::Differs(r) => Some((*m, r.clone())),
                        _ => None,
                    })
                    .unwrap();
                let how = if run.status == -1 {
                    "timeout".to_owned()
                } else if run.status > 128 && run.status != expected.status {
                    format!("signal-{}", run.status - 128)
                } else if run.stdout != expected.stdout {
                    "output".to_owned()
                } else if run.stderr != expected.stderr {
                    "stderr".to_owned()
                } else {
                    "exit-status".to_owned()
                };
                let class = if how == "timeout" {
                    "run-timeout"
                } else {
                    "miscompile"
                };
                Some(Signature::new(class, &format!("{modes} {how}")))
            }
        }
    }
}

/// A failure class (`miscompile`, `crash`, ...) and what distinguishes
/// it within the class.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Signature {
    pub class: String,
    pub detail: String,
}

impl Signature {
    fn new(class: &str, detail: &str) -> Signature {
        Signature {
            class: class.to_owned(),
            detail: detail.to_owned(),
        }
    }
}

impl std::fmt::Display for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.class, self.detail)
    }
}

fn modes_name(modes: &[Mode]) -> String {
    match modes {
        [Mode::Debug] => "debug".to_owned(),
        [Mode::Opt] => "opt".to_owned(),
        _ => "debug+opt".to_owned(),
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned()
}

/// The first diagnostic's message, without its location.
fn first_error(s: &str) -> String {
    for line in s.lines() {
        if let Some(i) = line.find("error") {
            let rest = &line[i..];
            return rest.to_owned();
        }
    }
    first_line(s)
}

/// Where the compiler panicked, and the message.
fn panic_site(s: &str) -> String {
    let mut lines = s.lines();
    while let Some(line) = lines.next() {
        if let Some(i) = line.find("panicked at ") {
            let at = line[i + 12..].trim_end_matches(':');
            let message = lines.next().unwrap_or("");
            // Keep the message's start: it often names values.
            let message: String = message.chars().take(100).collect();
            return format!("{at}: {message}");
        }
    }
    first_line(s)
}

/// Replaces what varies between programs (names, numbers) so that one
/// bug gives one signature.
pub fn normalize(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                chars.next();
            }
            out.push('N');
        } else {
            out.push(c);
        }
    }
    // A place in a generated variable (`v3`, `p1.0`, `b2[_].a`) names the
    // variable, which says nothing about the bug.
    let parts: Vec<&str> = out.split('`').collect();
    let mut joined = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            joined.push('`');
        }
        let generated_place = i % 2 == 1
            && part.starts_with(|c: char| c.is_ascii_lowercase())
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '[' | ']' | '_'))
            && part.contains('N');
        joined.push_str(if generated_place { "x" } else { part });
    }
    joined
}

/// Runs a command with a time limit, returning its output and status
/// (-1 when it ran out of time).
fn timed(cmd: &mut Command, seconds: u32) -> std::io::Result<(i32, String, String)> {
    let output = cmd.stdin(Stdio::null()).output()?;
    let status = match output.status.code() {
        Some(124) => -1,
        Some(c) => c,
        None => {
            use std::os::unix::process::ExitStatusExt;
            128 + output.status.signal().unwrap_or(0)
        }
    };
    let _ = seconds;
    Ok((
        status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

fn gtimeout(seconds: u32, program: &Path) -> Command {
    let mut c = Command::new("gtimeout");
    c.arg("-k").arg("2").arg(format!("{seconds}s")).arg(program);
    c
}

/// Runs an executable under `gtimeout`.
pub fn run(tools: &Tools, exe: &Path) -> Result<Run, String> {
    let (status, stdout, stderr) = timed(&mut gtimeout(tools.run_timeout, exe), tools.run_timeout)
        .map_err(|e| format!("cannot run {}: {e}", exe.display()))?;
    Ok(Run {
        stdout,
        stderr: mask_thread_id(&stderr),
        status,
    })
}

/// `stderr` with the thread id of `thread 'main' (123) panicked`
/// replaced by `N`.
fn mask_thread_id(stderr: &str) -> String {
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

/// Compiles `src` with rustc into `exe`: the error output if it fails.
pub fn rustc(tools: &Tools, src: &Path, exe: &Path) -> Result<(), String> {
    let mut cmd = gtimeout(tools.compile_timeout, Path::new(&tools.rustc));
    cmd.args([
        "--edition",
        "2024",
        "-C",
        "overflow-checks=off",
        "-C",
        "panic=abort",
    ])
    .arg("-o")
    .arg(exe)
    .arg(src);
    let (status, out, err) =
        timed(&mut cmd, tools.compile_timeout).map_err(|e| format!("cannot start rustc: {e}"))?;
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{out}{err}"))
    }
}

/// Builds `src` with flux into `exe`.
fn flux_build(tools: &Tools, src: &Path, exe: &Path, mode: Mode) -> Result<(), FluxOutcome> {
    let mut cmd = gtimeout(tools.compile_timeout, &tools.flux);
    cmd.arg("build").arg(src).arg("-o").arg(exe);
    if mode == Mode::Opt {
        cmd.arg("-O");
    }
    let (status, out, err) = timed(&mut cmd, tools.compile_timeout)
        .map_err(|e| FluxOutcome::Crashed(format!("cannot start flux: {e}")))?;
    let text = format!("{out}{err}");
    match status {
        0 => Ok(()),
        -1 => Err(FluxOutcome::CompileTimeout),
        // Diagnostics about the program.
        1 => Err(FluxOutcome::Rejected(text)),
        // `error: ...` from the pipeline after checking: keel, linking.
        2 => Err(FluxOutcome::Backend(text)),
        // A panic is 101; a signal, 128 + it.
        _ => Err(FluxOutcome::Crashed(if text.trim().is_empty() {
            format!("flux exited with status {status}")
        } else {
            text
        })),
    }
}

/// What flux alone says about `text`, building but not running it: the
/// signature of a compile-time failure (a rejection, crash, backend error
/// or timeout), or `None` if both builds succeed. Cheap, for the reducer
/// to discard candidates before asking rustc.
pub fn flux_compile_signature(
    tools: &Tools,
    dir: &Path,
    name: &str,
    text: &str,
) -> Option<Signature> {
    let src = dir.join(format!("{name}.rs"));
    std::fs::write(&src, text).ok()?;
    let mut flux = Vec::new();
    for mode in Mode::BOTH {
        let exe = dir.join(format!("{name}-flux-{}", mode.name()));
        let outcome = match flux_build(tools, &src, &exe, mode) {
            Ok(()) => FluxOutcome::Same,
            Err(o) => o,
        };
        let _ = std::fs::remove_file(&exe);
        flux.push((mode, outcome));
    }
    let expected = Run {
        stdout: String::new(),
        stderr: String::new(),
        status: 0,
    };
    Verdict::Tested { expected, flux }.signature()
}

/// Whether a signature's failure shows when compiling, so that
/// [`flux_compile_signature`] sees it.
pub fn is_compile_time(sig: &Signature) -> bool {
    matches!(
        sig.class.as_str(),
        "rejected" | "crash" | "backend" | "compile-timeout"
    )
}

/// Compiles and runs `text` every way, in `dir` (which it may fill with
/// files named after `name`).
pub fn test_program(tools: &Tools, dir: &Path, name: &str, text: &str, modes: &[Mode]) -> Verdict {
    let src = dir.join(format!("{name}.rs"));
    if let Err(e) = std::fs::write(&src, text) {
        return Verdict::InvalidProgram(format!("cannot write {}: {e}", src.display()));
    }
    let reference = dir.join(format!("{name}-rustc"));
    if let Err(e) = rustc(tools, &src, &reference) {
        return Verdict::InvalidProgram(e);
    }
    let expected = match run(tools, &reference) {
        Ok(r) => r,
        Err(e) => return Verdict::InvalidProgram(e),
    };
    let _ = std::fs::remove_file(&reference);
    if expected.status == -1 {
        return Verdict::ReferenceTimeout;
    }
    let mut flux = Vec::new();
    for &mode in modes {
        let exe = dir.join(format!("{name}-flux-{}", mode.name()));
        let outcome = match flux_build(tools, &src, &exe, mode) {
            Err(o) => o,
            Ok(()) => match run(tools, &exe) {
                Ok(r) if r == expected => FluxOutcome::Same,
                Ok(r) => FluxOutcome::Differs(r),
                Err(e) => FluxOutcome::Crashed(e),
            },
        };
        let _ = std::fs::remove_file(&exe);
        flux.push((mode, outcome));
    }
    Verdict::Tested { expected, flux }
}

/// A human-readable account of a verdict, for a failing case's
/// directory.
pub fn describe(v: &Verdict) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    match v {
        Verdict::InvalidProgram(m) => {
            let _ = writeln!(s, "rustc rejected the program (a generator bug):\n{m}");
        }
        Verdict::ReferenceTimeout => {
            let _ = writeln!(s, "rustc's program did not finish (a generator bug)");
        }
        Verdict::Tested { expected, flux } => {
            let _ = writeln!(
                s,
                "rustc: status {}, {} lines of output",
                expected.status,
                expected.stdout.lines().count()
            );
            for (mode, o) in flux {
                let _ = write!(s, "flux {}: ", mode.name());
                match o {
                    FluxOutcome::Same => {
                        let _ = writeln!(s, "same");
                    }
                    FluxOutcome::Differs(r) => {
                        let _ = writeln!(
                            s,
                            "status {}, {} lines of output",
                            r.status,
                            r.stdout.lines().count()
                        );
                        let _ = writeln!(s, "{}", first_difference(&expected.stdout, &r.stdout));
                        if r.stderr != expected.stderr {
                            let _ = writeln!(
                                s,
                                "  stderr: rustc {:?}\n          flux  {:?}",
                                expected.stderr, r.stderr
                            );
                        }
                    }
                    FluxOutcome::Rejected(m) => {
                        let _ = writeln!(s, "rejected:\n{m}");
                    }
                    FluxOutcome::Crashed(m) => {
                        let _ = writeln!(s, "crashed:\n{m}");
                    }
                    FluxOutcome::Backend(m) => {
                        let m: String = m.chars().take(4000).collect();
                        let _ = writeln!(s, "backend error:\n{m}");
                    }
                    FluxOutcome::CompileTimeout => {
                        let _ = writeln!(s, "compilation timed out");
                    }
                }
            }
        }
    }
    s
}

/// The first line where two outputs differ, with both versions.
pub fn first_difference(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    for i in 0..e.len().max(a.len()) {
        let x = e.get(i).copied();
        let y = a.get(i).copied();
        if x != y {
            return format!(
                "  first difference at line {}: rustc {:?}, flux {:?}",
                i + 1,
                x.unwrap_or("<end>"),
                y.unwrap_or("<end>")
            );
        }
    }
    "  same output".to_owned()
}
