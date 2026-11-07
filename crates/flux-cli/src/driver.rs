use std::path::PathBuf;
use std::process::ExitCode;

use crate::{build, builtins, cargo, flib_build};

use clap::{Parser, Subcommand};
use flux_hir::{Diagnostic, Hir, SourceDb};
use flux_syntax::{LexedStr, TextRange};

#[derive(Parser)]
#[command(name = "fluxc", version, about = "The flux compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    BuildUnit { request: PathBuf },
    #[command(hide = true)]
    PackageDriver {
        #[command(subcommand)]
        command: cargo::Command,
    },
    /// Print native compiler configuration.
    Print {
        #[arg(value_enum)]
        kind: PrintKind,
    },
    /// Stage checked built-in types and MIR beside the compiler executable.
    BuildBuiltins {
        /// Include every non-generic built-in definition in one native object.
        #[arg(long)]
        native_object: bool,
        #[arg(short = 'O')]
        optimize: bool,
    },
    /// Print the tokens of a file, trivia included.
    Lex { file: PathBuf },
    /// Print the syntax tree of a file.
    Parse { file: PathBuf },
    /// Print the HIR of a file: its items and lowered function bodies.
    Hir { file: PathBuf },
    /// Type-check a file and print the MIR of its functions.
    Mir { file: PathBuf },
    /// Report syntax errors in files.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Compile a program or a Flux library.
    Build {
        file: PathBuf,
        /// Compile an executable or a private Fluxlib library.
        #[arg(long, value_enum, default_value = "bin")]
        crate_type: CrateType,
        /// Explicit crate identity name.
        #[arg(long)]
        crate_name: Option<String>,
        /// Write metadata without native code (libraries only).
        #[arg(long)]
        metadata_only: bool,
        /// Load an external Flux library as alias=path.
        #[arg(long = "extern", value_parser = parse_extern)]
        externs: Vec<(String, PathBuf)>,
        /// Where to write the executable; the file's name without `.rs` by
        /// default.
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        options: BuildOptions,
    },
    /// Compile a program and run it, exiting with its exit code.
    Run {
        file: PathBuf,
        #[command(flatten)]
        options: BuildOptions,
        /// Arguments for the program.
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum PrintKind {
    Host,
    Cfg,
}

#[derive(clap::ValueEnum, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrateType {
    Bin,
    Lib,
}

#[derive(clap::Args, Clone, Copy)]
pub(crate) struct BuildOptions {
    /// Optimize with keel's full pipeline.
    #[arg(short = 'O')]
    pub(crate) optimize: bool,
    /// Print scoped compilation timings (excludes execution of the program).
    #[arg(long)]
    pub(crate) timings: bool,
    /// Codegen option, as rustc's: only `debug-assertions[=yes|no]`, which
    /// is on by default without `-O` and off with it.
    #[arg(
        short = 'C',
        value_name = "OPT[=VALUE]",
        value_parser = parse_codegen_option,
        overrides_with = "debug_assertions"
    )]
    pub(crate) debug_assertions: Option<bool>,
}

impl BuildOptions {
    /// Whether `cfg(debug_assertions)` holds for the compiled crates.
    pub(crate) fn debug_assertions(&self) -> bool {
        self.debug_assertions.unwrap_or(!self.optimize)
    }
}

/// Parses `-C debug-assertions[=VALUE]` with rustc's boolean spellings.
fn parse_codegen_option(text: &str) -> Result<bool, String> {
    let (name, value) = text.split_once('=').unwrap_or((text, "yes"));
    if name != "debug-assertions" {
        return Err(format!("unsupported codegen option `{name}`"));
    }
    match value {
        "y" | "yes" | "on" | "true" => Ok(true),
        "n" | "no" | "off" | "false" => Ok(false),
        _ => Err(format!(
            "expected `yes` or `no` for `debug-assertions`, found `{value}`"
        )),
    }
}

/// Run the source compiler CLI.
pub fn compiler_main() -> ExitCode {
    if std::env::var_os("FLUX_COMPILER_PROBE").is_some() {
        return crate::probe::run();
    }
    let cli = Cli::parse();
    if let Command::Print { kind } = &cli.command {
        let result = match kind {
            PrintKind::Host => crate::target::host().map(|host| println!("{host}")),
            PrintKind::Cfg => crate::target::cfg(true).map(|cfg| {
                for line in cfg {
                    println!("{line}");
                }
            }),
        };
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::from(2)
            }
        };
    }
    if let Command::PackageDriver { command } = &cli.command {
        return execute_package(command);
    }
    if !matches!(
        cli.command,
        Command::BuildBuiltins { .. }
            | Command::Lex { .. }
            | Command::Parse { .. }
            | Command::Check { .. }
    ) && let Err(message) = crate::builtins::initialize()
    {
        eprintln!("error: {message}");
        return ExitCode::from(2);
    }
    if let Command::Run {
        file,
        options,
        args,
    } = &cli.command
    {
        return match build::run(file, *options, args) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::from(2)
            }
        };
    }
    match run(cli.command) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

/// Runs a command. `Ok(false)` means the input had errors.
fn run(command: Command) -> Result<bool, String> {
    match command {
        Command::BuildUnit { request } => cargo::native::build::worker(&request),
        Command::BuildBuiltins {
            native_object,
            optimize,
        } => {
            let path = crate::builtins::stage(native_object, optimize)?;
            println!("{}", path.display());
            Ok(true)
        }
        Command::Lex { file } => {
            let text = read(&file)?;
            let lexed = LexedStr::new(&text);
            for i in 0..lexed.len() {
                println!(
                    "{:?}@{:?} {:?}",
                    lexed.kind(i),
                    lexed.token_range(i),
                    lexed.token_text(i)
                );
            }
            for error in lexed.errors() {
                let range = lexed.token_range(error.token);
                report(&file, &text, range, &error.message);
            }
            Ok(lexed.errors().is_empty())
        }
        Command::Parse { file } => {
            let text = read(&file)?;
            let parse = flux_syntax::parse_source_file(&text);
            print!("{:#?}", parse.root());
            for error in parse.errors() {
                report(&file, &text, error.range, &error.message);
            }
            Ok(parse.errors().is_empty())
        }
        Command::Hir { file } => {
            let (db, hir) = load(&file)?;
            print!("{}", hir.debug_dump());
            let mut errors = db.syntax_errors();
            errors.extend(crate::identifiers::diagnostics(&db));
            errors.extend(hir.diagnostics.iter().cloned());
            Ok(report_all(&db, &mut errors))
        }
        Command::Mir { file } => {
            let (db, hir) = load(&file)?;
            let providers = crate::builtins::active()
                .map(|s| s.providers())
                .transpose()?;
            let roots = providers.as_ref().map(|(loaded, imported)| {
                imported.required_functions(&hir.items, loaded.signatures())
            });
            let typed = if let Some((loaded, imported)) = &providers {
                flux_mir::check_file_from(
                    &hir,
                    loaded,
                    imported,
                    &roots.as_ref().expect("provider roots").functions,
                )
            } else {
                flux_mir::check_file(&hir)
            };
            let mut errors = db.syntax_errors();
            errors.extend(crate::identifiers::diagnostics(&db));
            errors.extend(hir.diagnostics.iter().cloned());
            errors.extend(typed.diagnostics.iter().cloned());
            if !errors.is_empty() {
                let more = if let Some((_, imported)) = &providers {
                    flux_mir::check_clean_bodies_from(&hir, &typed, &errors, imported)
                } else {
                    flux_mir::check_clean_bodies(&hir, &typed, &errors)
                };
                errors.extend(more);
                return Ok(report_all(&db, &mut errors));
            }
            let mir = if let Some((_, imported)) = &providers {
                flux_mir::build_file_from(
                    &hir,
                    &typed,
                    imported,
                    &roots.as_ref().expect("provider roots").functions,
                )
            } else {
                flux_mir::build_file(&hir, &typed)
            };
            print!("{}", mir.debug_dump(&hir));
            let mut errors = mir.diagnostics.clone();
            Ok(report_all(&db, &mut errors))
        }
        Command::Build {
            file,
            output,
            options,
            crate_type,
            crate_name,
            metadata_only,
            externs,
        } => {
            let name = crate_name.unwrap_or_else(|| build::crate_name(&file));
            if crate_type == CrateType::Lib {
                let output = output.unwrap_or_else(|| PathBuf::from(format!("{name}.flib")));
                flib_build::library(&file, &output, &name, options, metadata_only, &externs)
            } else {
                if metadata_only {
                    return Err("metadata-only output requires --crate-type lib".into());
                }
                let output = output.unwrap_or_else(|| PathBuf::from(&name));
                if !externs.is_empty() {
                    return flib_build::compile(
                        &build::SourceInput::standalone(&file, &name),
                        &output,
                        options,
                        CrateType::Bin,
                        false,
                        &externs,
                    );
                }
                build::compile(build::CompileRequest {
                    file: &file,
                    crate_name: &name,
                    output: &output,
                    options,
                    sources: &[],
                    native: Default::default(),
                })
            }
        }
        Command::Print { .. } | Command::PackageDriver { .. } | Command::Run { .. } => {
            unreachable!("handled in main")
        }
        Command::Check { files } => {
            let mut ok = true;
            for file in &files {
                let text = read(file)?;
                let parse = flux_syntax::parse_source_file(&text);
                for error in parse.errors() {
                    report(file, &text, error.range, &error.message);
                }
                ok &= parse.errors().is_empty();
            }
            Ok(ok)
        }
    }
}

/// Reads the program whose root is `file`, with its modules' files, and
/// lowers it.
fn load(file: &std::path::Path) -> Result<(SourceDb, Hir), String> {
    let suite = crate::builtins::active();
    let mut db = suite.map_or_else(SourceDb::new, |s| s.db.fork());
    let root = db.load(file)?;
    let name = flux_intern::Symbol::intern(&build::crate_name(file));
    let hir = if let Some(suite) = suite {
        flux_hir::lower_source_on(&mut db, suite.items.clone(), root, name, Vec::new())
    } else {
        flux_hir::lower(&mut db, root, name)
    };
    Ok((db, hir))
}

/// Reports `diagnostics` in order of file and position; whether there
/// were none.
pub(crate) fn report_all(db: &SourceDb, diagnostics: &mut [Diagnostic]) -> bool {
    for d in diagnostics.iter_mut() {
        db.resolve_diagnostic(d);
    }
    Diagnostic::sort(diagnostics);
    // Each expansion of a macro reports what is wrong in its definition
    // again, at the same place: once is enough.
    let mut seen = std::collections::HashSet::new();
    for d in diagnostics.iter() {
        if !seen.insert((d.file, d.range, &d.message)) {
            continue;
        }
        let f = db.file(d.file);
        report(&f.path, f.text(), d.range, &d.message);
    }
    diagnostics.is_empty()
}

fn read(file: &PathBuf) -> Result<String, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    // Like rustc, ignore a leading byte order mark.
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_owned(),
        None => text,
    })
}

/// Prints `file:line:column: error: message`.
pub(crate) fn report(file: &std::path::Path, text: &str, range: TextRange, message: &str) {
    let (line, column) = line_column(text, range.start());
    eprintln!("{}:{line}:{column}: error: {message}", file.display());
}

/// One-based line and column (in characters) of `offset`.
fn line_column(text: &str, offset: u32) -> (usize, usize) {
    let before = &text[..offset as usize];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() + 1;
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

fn parse_extern(value: &str) -> Result<(String, PathBuf), String> {
    let (name, path) = value
        .split_once('=')
        .ok_or("expected alias=path for --extern")?;
    if name.is_empty()
        || path.is_empty()
        || !name
            .chars()
            .enumerate()
            .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
    {
        return Err("expected a crate alias and artifact path".into());
    }
    Ok((name.to_owned(), PathBuf::from(path)))
}

#[derive(Parser)]
#[command(name = "flux", version, about = "The Flux package manager")]
struct PackageCli {
    #[command(subcommand)]
    command: cargo::Command,
}

/// Run the package manager CLI.
pub fn package_main() -> ExitCode {
    let _cli = PackageCli::parse();
    let compiler = match std::env::current_exe() {
        Ok(path) => path.with_file_name("fluxc"),
        Err(error) => {
            eprintln!("error: cannot locate fluxc: {error}");
            return ExitCode::from(2);
        }
    };
    match std::process::Command::new(&compiler)
        .arg("package-driver")
        .args(std::env::args_os().skip(1))
        .status()
    {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
        Err(error) => {
            eprintln!(
                "error: cannot run {}: {error}; install flux and fluxc together",
                compiler.display()
            );
            ExitCode::from(2)
        }
    }
}

fn execute_package(command: &cargo::Command) -> ExitCode {
    let initialize = matches!(
        command,
        cargo::Command::Build { .. } | cargo::Command::Package { .. }
    );
    let result = if initialize {
        builtins::initialize().and_then(|()| cargo::execute(command))
    } else {
        cargo::execute(command)
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}
