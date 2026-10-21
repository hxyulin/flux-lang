use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flux_syntax::{LexedStr, TextRange};

#[derive(Parser)]
#[command(name = "flux", version, about = "The flux compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the tokens of a file, trivia included.
    Lex { file: PathBuf },
    /// Print the syntax tree of a file.
    Parse { file: PathBuf },
    /// Report syntax errors in files.
    Check {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
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

fn read(file: &PathBuf) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))
}

/// Prints `file:line:column: error: message`.
fn report(file: &std::path::Path, text: &str, range: TextRange, message: &str) {
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
