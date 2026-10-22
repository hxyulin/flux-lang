//! `flux build` and `flux run`: the whole pipeline, from source to an
//! executable.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use flux_codegen::{OptimizationTier, Target};
use flux_hir::SourceDb;
use flux_intern::Symbol;

use crate::{BuildOptions, report_all};

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
    compile(CompileRequest {
        file,
        crate_name: &crate_name(file),
        output,
        options,
        sources: &[],
    })
}

/// One source crate compiled with Flux's fixed edition-2024 host configuration.
/// No Cargo profile, feature, target or rustc option is implied by this request.
pub(crate) struct CompileRequest<'a> {
    pub file: &'a Path,
    pub crate_name: &'a str,
    pub output: &'a Path,
    pub options: BuildOptions,
    /// Dependency-ordered source crates; empty for a single source file.
    pub sources: &'a [SourceInput],
}

pub(crate) struct SourceInput {
    pub file: std::path::PathBuf,
    pub name: String,
    pub dependencies: Vec<(String, usize)>,
}

pub(crate) fn compile(request: CompileRequest<'_>) -> Result<bool, String> {
    let CompileRequest {
        file,
        crate_name,
        output,
        options,
        sources,
    } = request;
    let mut timings = Timings::default();
    // `core`'s files are parsed with the database.
    let (mut db, root) = timings.time("parse", || {
        let mut db = SourceDb::new();
        let root = db.load(file);
        (db, root)
    });
    let root = root?;
    let mut graph = Vec::new();
    for source in sources {
        graph.push(flux_hir::SourceCrate {
            root: db.load(&source.file)?,
            name: Symbol::intern(&source.name),
            dependencies: source
                .dependencies
                .iter()
                .map(|(alias, target)| (Symbol::intern(alias), *target))
                .collect(),
        });
    }
    let hir = timings.time("hir", || {
        if graph.is_empty() {
            flux_hir::lower(&mut db, root, Symbol::intern(crate_name))
        } else {
            flux_hir::lower_sources(&mut db, &graph)
        }
    });
    let typed = timings.time("types", || flux_mir::check_file(&hir));
    let mut errors = db.syntax_errors();
    errors.extend(hir.diagnostics.iter().cloned());
    errors.extend(typed.diagnostics.iter().cloned());
    if let Some(d) = flux_ty::check_main(&hir.items, &typed.sigs) {
        errors.push(d);
    }
    if !errors.is_empty() {
        let more = flux_mir::check_clean_bodies(&hir, &typed, &errors);
        errors.extend(more);
    } else {
        let mir = timings.time("mir", || flux_mir::build_file(&hir, &typed));
        errors.extend(mir.diagnostics.iter().cloned());
        if errors.is_empty() {
            let target = Target::host()
                .ok_or("flux can only compile for arm64 macOS and x86-64 Linux hosts")?;
            let mut module =
                match timings.time("lower", || flux_codegen::lower_module(&hir, &typed, &mir)) {
                    Ok(module) => module,
                    Err(found) => {
                        errors.extend(found);
                        report_all(&db, &mut errors);
                        return Ok(false);
                    }
                };
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
    report_all(&db, &mut errors);
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

pub(crate) fn exit_code(status: std::process::ExitStatus) -> u8 {
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
