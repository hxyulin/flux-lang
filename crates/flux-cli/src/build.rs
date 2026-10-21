//! `flux build` and `flux run`: the whole pipeline, from source to an
//! executable.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use flux_codegen::{OptimizationTier, Target};
use flux_syntax::TextRange;

use crate::{BuildOptions, read, report};

/// The crate name for a file: its stem, with anything that cannot appear
/// in an identifier replaced by `_`.
pub(crate) fn crate_name(file: &Path) -> String {
    let stem = file
        .file_stem()
        .map_or("main".into(), |s| s.to_string_lossy());
    let name: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// Phase timings, printed with `--timings`.
#[derive(Default)]
struct Timings(Vec<(&'static str, Duration)>);

impl Timings {
    fn time<T>(&mut self, phase: &'static str, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let result = f();
        self.0.push((phase, start.elapsed()));
        result
    }

    fn print(&self) {
        let total: Duration = self.0.iter().map(|(_, d)| *d).sum();
        for (phase, duration) in &self.0 {
            eprintln!("{phase:>10}  {duration:>10.2?}");
        }
        eprintln!("{:>10}  {total:>10.2?}", "total");
    }
}

/// Compiles `file` to an executable at `output`. `Ok(false)` means the
/// program had errors, which were reported.
pub(crate) fn build(file: &Path, output: &Path, options: BuildOptions) -> Result<bool, String> {
    let text = read(&file.to_path_buf())?;
    let mut timings = Timings::default();
    let parse = timings.time("parse", || flux_syntax::parse_source_file(&text));
    let hir = timings.time("hir", || flux_hir::lower_file(&parse));
    let typed = timings.time("types", || flux_ty::check_file(&hir));
    let mut errors: Vec<(TextRange, String)> = parse
        .errors()
        .iter()
        .map(|e| (e.range, e.message.clone()))
        .chain(hir.diagnostics.iter().map(|d| (d.range, d.message.clone())))
        .chain(
            typed
                .diagnostics
                .iter()
                .map(|d| (d.range, d.message.clone())),
        )
        .collect();
    if let Some(d) = flux_ty::check_main(&hir.items, &typed.sigs) {
        errors.push((d.range, d.message));
    }
    if errors.is_empty() {
        let mir = timings.time("mir", || flux_mir::build_file(&hir, &typed));
        errors.extend(mir.diagnostics.iter().map(|d| (d.range, d.message.clone())));
        if errors.is_empty() {
            let target = Target::host()
                .ok_or("flux can only compile for arm64 macOS and x86-64 Linux hosts")?;
            let crate_name = crate_name(file);
            let mut module = timings.time("lower", || {
                flux_codegen::lower_module(&hir, &typed, &mir, &crate_name)
            });
            let tier = if options.optimize {
                OptimizationTier::Full
            } else {
                OptimizationTier::None
            };
            let object = timings.time("codegen", || {
                flux_codegen::compile_object(&mut module, target, tier)
            })?;
            let warnings = timings.time("link", || {
                flux_codegen::link_executable(&object, target, output)
            })?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
        }
    }
    errors.sort_by_key(|(range, _)| (range.start(), range.end()));
    for (range, message) in &errors {
        report(file, &text, *range, message);
    }
    if options.timings {
        timings.print();
    }
    Ok(errors.is_empty())
}

/// Builds `file` into a temporary executable and runs it with `args`,
/// returning its exit code (128 plus the signal if one killed it).
pub(crate) fn run(file: &Path, options: BuildOptions, args: &[String]) -> Result<ExitCode, String> {
    let output = std::env::temp_dir().join(format!(
        "flux-run-{}-{}",
        std::process::id(),
        crate_name(file)
    ));
    if !build(file, &output, options)? {
        return Ok(ExitCode::FAILURE);
    }
    let status = std::process::Command::new(&output).args(args).status();
    let _ = std::fs::remove_file(&output);
    let status = status.map_err(|e| format!("cannot run {}: {e}", output.display()))?;
    Ok(ExitCode::from(exit_code(status)))
}

fn exit_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return code as u8;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128u8.wrapping_add(signal as u8);
        }
    }
    1
}
