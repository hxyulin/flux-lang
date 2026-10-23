use std::path::PathBuf;
use std::process::ExitCode;

mod build;
mod package;

use clap::{Parser, Subcommand};
use flux_hir::{Diagnostic, Hir, SourceDb};
use flux_syntax::{LexedStr, TextRange};

#[derive(Parser)]
#[command(name = "flux", version, about = "The flux compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build or run a restricted standalone Cargo package with Flux.
    Package {
        #[command(subcommand)]
        command: package::Command,
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
    /// Compile a program to an executable.
    Build {
        file: PathBuf,
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

#[derive(clap::Args, Clone, Copy)]
struct BuildOptions {
    /// Optimize with keel's full pipeline.
    #[arg(short = 'O')]
    optimize: bool,
    /// Print scoped compilation timings (excludes execution of the program).
    #[arg(long)]
    timings: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Command::Package { command } = &cli.command {
        return match package::execute(command) {
            Ok(code) => code,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::from(2)
            }
        };
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
            errors.extend(hir.diagnostics.iter().cloned());
            Ok(report_all(&db, &mut errors))
        }
        Command::Mir { file } => {
            let (db, hir) = load(&file)?;
            let typed = flux_mir::check_file(&hir);
            let mut errors = db.syntax_errors();
            errors.extend(hir.diagnostics.iter().cloned());
            errors.extend(typed.diagnostics.iter().cloned());
            if !errors.is_empty() {
                let more = flux_mir::check_clean_bodies(&hir, &typed, &errors);
                errors.extend(more);
                return Ok(report_all(&db, &mut errors));
            }
            let mir = flux_mir::build_file(&hir, &typed);
            print!("{}", mir.debug_dump(&hir));
            let mut errors = mir.diagnostics.clone();
            Ok(report_all(&db, &mut errors))
        }
        Command::Build {
            file,
            output,
            options,
        } => {
            let output = output.unwrap_or_else(|| PathBuf::from(build::crate_name(&file)));
            build::build(&file, &output, options)
        }
        Command::Package { .. } | Command::Run { .. } => unreachable!("handled in main"),
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
    let mut db = SourceDb::new();
    let root = db.load(file)?;
    let name = flux_intern::Symbol::intern(&build::crate_name(file));
    let hir = flux_hir::lower(&mut db, root, name);
    Ok((db, hir))
}

/// Reports `diagnostics` in order of file and position; whether there
/// were none.
pub(crate) fn report_all(db: &SourceDb, diagnostics: &mut [Diagnostic]) -> bool {
    for d in diagnostics.iter_mut() {
        d.file = db.real_file(d.file);
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
    std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))
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
