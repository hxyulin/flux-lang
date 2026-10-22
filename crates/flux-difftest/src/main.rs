//! `difftest`: differential testing of flux against rustc.
//!
//! ```text
//! difftest gen 42                  # print the program for seed 42
//! difftest test 42                 # compile and compare it, print the verdict
//! difftest run -n 2000             # a campaign over seeds 0..2000
//! difftest reduce 42               # minimize seed 42's failure
//! difftest reduce --file bug.rs    # minimize a program by deleting lines
//! ```
//!
//! Campaigns write to `out/difftest/` (see `--out`): `cases/<seed>/`
//! holds each failing program, what happened and its reduction, and
//! `summary.txt` the failures grouped by signature.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use flux_difftest::bisect;
use flux_difftest::driver::{self, Mode, Signature, Tools, Verdict};
use flux_difftest::generator::{Config, Weights};
use flux_difftest::reduce::{self, Reducer};

#[derive(Parser)]
#[command(
    name = "difftest",
    about = "Differential testing of flux against rustc"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone)]
struct GenOptions {
    /// Set a feature weight, `name=N` (0 turns it off); repeatable.
    #[arg(long = "weight", value_name = "NAME=N")]
    weights: Vec<String>,
}

impl GenOptions {
    fn config(&self) -> Result<Config, String> {
        let mut cfg = Config::default();
        for w in &self.weights {
            let (name, value) = w
                .split_once('=')
                .ok_or_else(|| format!("expected `name=N`, found `{w}`"))?;
            let value: u32 = value
                .parse()
                .map_err(|_| format!("not a number: `{value}`"))?;
            cfg.weights.set(name, value)?;
        }
        Ok(cfg)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Print the program generated from a seed.
    Gen {
        seed: u64,
        #[command(flatten)]
        options: GenOptions,
    },
    /// Compile one seed's program (or a file) every way and report.
    Test {
        seed: Option<u64>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[command(flatten)]
        options: GenOptions,
    },
    /// Run a campaign: generate, compile, run and classify many programs.
    Run {
        /// How many programs.
        #[arg(short = 'n', default_value_t = 1000)]
        count: u64,
        /// The first seed.
        #[arg(long, default_value_t = 0)]
        start: u64,
        /// Parallel jobs (all cores by default).
        #[arg(short = 'j')]
        jobs: Option<usize>,
        #[arg(long, default_value = "out/difftest")]
        out: PathBuf,
        /// Reduce the first case of each distinct failure.
        #[arg(long)]
        reduce: bool,
        #[command(flatten)]
        options: GenOptions,
    },
    /// Minimize a failing seed (by its choices, then lines) or file (by
    /// lines), keeping its failure signature.
    Reduce {
        seed: Option<u64>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(short = 'j')]
        jobs: Option<usize>,
        #[arg(long, default_value = "out/difftest")]
        out: PathBuf,
        /// Where to write the result (`<out>/cases/<seed>/reduced.rs` by
        /// default).
        #[arg(short = 'o')]
        output: Option<PathBuf>,
        #[command(flatten)]
        options: GenOptions,
    },
    /// For a program that fails only with `-O`: find the first pass of
    /// keel's pipeline after which it differs from rustc's.
    BisectOpt {
        file: PathBuf,
        /// Write the keel IR before and after that pass to `<out>/bisect/`.
        #[arg(long)]
        dump: bool,
        #[arg(long, default_value = "out/difftest")]
        out: PathBuf,
    },
    /// Print flux's keel IR for a program, after the first N passes of
    /// keel's `-O` pipeline (none by default).
    Ir {
        file: PathBuf,
        #[arg(long, default_value_t = 0)]
        passes: usize,
    },
    /// List the feature weights and their defaults.
    Weights,
    /// Check that rustc accepts (and can run) generated programs: for
    /// working on the generator.
    Validate {
        #[arg(short = 'n', default_value_t = 200)]
        count: u64,
        #[arg(long, default_value_t = 0)]
        start: u64,
        #[arg(short = 'j')]
        jobs: Option<usize>,
        #[command(flatten)]
        options: GenOptions,
    },
}

fn main() -> std::process::ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn jobs(j: Option<usize>) -> usize {
    j.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Gen { seed, options } => {
            let (text, _) = flux_difftest::program(seed, &options.config()?);
            print!("{text}");
            Ok(())
        }
        Command::Validate {
            count,
            start,
            jobs: j,
            options,
        } => validate(count, start, jobs(j), &options.config()?),
        Command::BisectOpt { file, dump, out } => bisect_opt(&file, dump, &out),
        Command::Ir { file, passes } => {
            let mut module = bisect::lower(&file)?;
            bisect::optimize(&mut module, passes.min(bisect::pipeline().len()))?;
            print!("{module}");
            Ok(())
        }
        Command::Weights => {
            let d = format!("{:#?}", Weights::default());
            println!("{d}");
            Ok(())
        }
        Command::Test {
            seed,
            file,
            options,
        } => {
            let text = source(seed, file.as_deref(), &options.config()?)?;
            let tools = Tools::find()?;
            let dir = PathBuf::from("out/difftest/test");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let v = driver::test_program(&tools, &dir, "test", &text, &Mode::BOTH);
            print!("{}", driver::describe(&v));
            match v.signature() {
                Some(s) => println!("signature: {s}"),
                None => println!("ok"),
            }
            Ok(())
        }
        Command::Run {
            count,
            start,
            jobs: j,
            out,
            reduce,
            options,
        } => campaign(count, start, jobs(j), &out, reduce, &options),
        Command::Reduce {
            seed,
            file,
            jobs: j,
            out,
            output,
            options,
        } => {
            let cfg = options.config()?;
            let tools = Tools::find()?;
            let name = match (seed, &file) {
                (Some(s), _) => s.to_string(),
                (None, Some(f)) => f
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                _ => return Err("give a seed or --file".to_owned()),
            };
            let output = output.unwrap_or_else(|| out.join("cases").join(&name).join("reduced.rs"));
            let reduced = reduce_one(
                &tools,
                &cfg,
                seed,
                file.as_deref(),
                &out,
                &name,
                jobs(j),
                true,
            )?;
            if let Some(dir) = output.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::write(&output, &reduced.0).map_err(|e| e.to_string())?;
            println!("{}", reduced.0);
            eprintln!("signature: {}\nwritten to {}", reduced.1, output.display());
            Ok(())
        }
    }
}

fn source(seed: Option<u64>, file: Option<&Path>, cfg: &Config) -> Result<String, String> {
    match (seed, file) {
        (_, Some(f)) => std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display())),
        (Some(s), None) => Ok(flux_difftest::program(s, cfg).0),
        (None, None) => Err("give a seed or --file".to_owned()),
    }
}

/// Reduces one case; returns the program and its signature.
#[allow(clippy::too_many_arguments)]
fn reduce_one(
    tools: &Tools,
    cfg: &Config,
    seed: Option<u64>,
    file: Option<&Path>,
    out: &Path,
    name: &str,
    jobs: usize,
    verbose: bool,
) -> Result<(String, Signature), String> {
    let text = source(seed, file, cfg)?;
    let dir = reduce::scratch(out, name);
    let v = driver::test_program(tools, &dir, "original", &text, &Mode::BOTH);
    let sig = v
        .signature()
        .ok_or("the program does not fail: nothing to reduce")?;
    if sig.class == "invalid-program" {
        return Err(format!(
            "rustc rejects the program, so it cannot be reduced: {sig}"
        ));
    }
    let mut r = Reducer::new(tools.clone(), cfg.clone(), sig.clone(), dir, jobs);
    r.verbose = verbose;
    let tape = match (seed, file) {
        (Some(s), None) => Some(flux_difftest::program(s, cfg).1),
        _ => None,
    };
    let start = Instant::now();
    let reduced = reduce::reduce(&r, tape, text);
    if verbose {
        eprintln!("reduce: {} tests in {:.0?}", r.tests(), start.elapsed());
    }
    Ok((reduced, sig))
}

/// Compiles programs with rustc only, runs them, and reports those it
/// rejects or that do not finish, grouped by the first error.
fn validate(count: u64, start: u64, jobs: usize, cfg: &Config) -> Result<(), String> {
    let mut tools = Tools::find().unwrap_or(Tools {
        flux: PathBuf::new(),
        rustc: "rustc".to_owned(),
        run_timeout: 10,
        compile_timeout: 60,
    });
    tools.run_timeout = 5;
    let next = AtomicU64::new(start);
    let end = start + count;
    let errors: Mutex<BTreeMap<String, Vec<u64>>> = Mutex::new(BTreeMap::new());
    let sizes = AtomicU64::new(0);
    std::thread::scope(|s| {
        for w in 0..jobs {
            let (next, errors, tools, sizes) = (&next, &errors, &tools, &sizes);
            s.spawn(move || {
                let dir = PathBuf::from("out/difftest/validate").join(format!("w{w}"));
                let _ = std::fs::create_dir_all(&dir);
                loop {
                    let seed = next.fetch_add(1, Ordering::Relaxed);
                    if seed >= end {
                        break;
                    }
                    let (text, _) = flux_difftest::program(seed, cfg);
                    sizes.fetch_add(text.lines().count() as u64, Ordering::Relaxed);
                    let src = dir.join(format!("{seed}.rs"));
                    let exe = dir.join(format!("{seed}"));
                    let _ = std::fs::write(&src, &text);
                    let problem = match driver::rustc(tools, &src, &exe) {
                        Err(e) => Some(
                            e.lines()
                                .find(|l| l.starts_with("error"))
                                .unwrap_or("rustc failed")
                                .to_owned(),
                        ),
                        Ok(()) => match driver::run(tools, &exe) {
                            Ok(r) if r.status == -1 => Some("timeout".to_owned()),
                            Ok(r) if r.stdout.lines().count() > 200_000 => {
                                Some("huge output".to_owned())
                            }
                            Ok(_) => None,
                            Err(e) => Some(e),
                        },
                    };
                    let _ = std::fs::remove_file(&exe);
                    match problem {
                        Some(p) => errors
                            .lock()
                            .unwrap()
                            .entry(driver::normalize(&p))
                            .or_default()
                            .push(seed),
                        None => {
                            let _ = std::fs::remove_file(&src);
                        }
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    let bad: usize = errors.values().map(Vec::len).sum();
    println!(
        "{count} programs, {bad} invalid, {} lines on average",
        sizes.load(Ordering::Relaxed) / count.max(1)
    );
    for (e, seeds) in &errors {
        let first: Vec<String> = seeds.iter().take(6).map(u64::to_string).collect();
        println!("{:5} {e}\n      seeds {}", seeds.len(), first.join(" "));
    }
    if bad == 0 {
        Ok(())
    } else {
        Err(format!("{bad} invalid programs"))
    }
}

/// Finds the first pass of keel's `-O` pipeline after which `file`
/// behaves differently from rustc's build of it.
fn bisect_opt(file: &Path, dump: bool, out: &Path) -> Result<(), String> {
    let tools = Tools::find()?;
    let dir = out.join("bisect");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let reference = dir.join("rustc");
    driver::rustc(&tools, file, &reference)?;
    let expected = driver::run(&tools, &reference)?;
    let passes = bisect::pipeline();
    let same = |n: usize| -> Result<bool, String> {
        let exe = dir.join(format!("flux-{n}"));
        bisect::build(file, &exe, n)?;
        let r = driver::run(&tools, &exe)?;
        let _ = std::fs::remove_file(&exe);
        Ok(r == expected)
    };
    if !same(0)? {
        println!(
            "differs from rustc with no optimization passes: the bug is in flux's lowering or keel's code generator, not keel's optimizer"
        );
        return Ok(());
    }
    if same(passes.len())? {
        println!(
            "matches rustc after all {} passes: not reproduced",
            passes.len()
        );
        return Ok(());
    }
    // `lo` passes match, `hi` passes differ.
    let (mut lo, mut hi) = (0, passes.len());
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if same(mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    println!(
        "matches rustc after {lo} passes, differs after {hi}: pass {hi} of {}, `{}`",
        passes.len(),
        passes[hi - 1]
    );
    if dump {
        let mut module = bisect::lower(file)?;
        bisect::optimize(&mut module, lo)?;
        std::fs::write(dir.join("ir-before.txt"), module.to_string()).map_err(|e| e.to_string())?;
        keel_opt_one(&mut module, passes[hi - 1])?;
        std::fs::write(dir.join("ir-after.txt"), module.to_string()).map_err(|e| e.to_string())?;
        println!(
            "keel IR before and after it: {}/ir-{{before,after}}.txt",
            dir.display()
        );
    }
    Ok(())
}

fn keel_opt_one(module: &mut keel_ir::Module, pass: &str) -> Result<(), String> {
    keel_opt::run_passes(module, &[pass]).map_err(|e| e.to_string())
}

/// One finished program in a campaign.
struct Outcome {
    seed: u64,
    signature: Option<Signature>,
}

fn campaign(
    count: u64,
    start: u64,
    jobs: usize,
    out: &Path,
    reduce: bool,
    options: &GenOptions,
) -> Result<(), String> {
    let cfg = options.config()?;
    let tools = Tools::find()?;
    let cases = out.join("cases");
    std::fs::create_dir_all(&cases).map_err(|e| e.to_string())?;
    let next = AtomicU64::new(start);
    let end = start + count;
    let results: Mutex<Vec<Outcome>> = Mutex::new(Vec::new());
    let begun = Instant::now();
    std::thread::scope(|s| {
        for w in 0..jobs {
            let (next, results, tools, cfg, cases) = (&next, &results, &tools, &cfg, &cases);
            s.spawn(move || {
                let dir = out.join("work").join(format!("w{w}"));
                let _ = std::fs::create_dir_all(&dir);
                loop {
                    let seed = next.fetch_add(1, Ordering::Relaxed);
                    if seed >= end {
                        break;
                    }
                    let (text, _) = flux_difftest::program(seed, cfg);
                    let v = driver::test_program(tools, &dir, "p", &text, &Mode::BOTH);
                    let signature = v.signature();
                    if let Some(sig) = &signature {
                        save_case(cases, seed, &text, &v, sig, options);
                    }
                    let mut r = results.lock().unwrap();
                    r.push(Outcome { seed, signature });
                    let done = r.len() as u64;
                    if done.is_multiple_of(100) || done == count {
                        let failed = r.iter().filter(|o| o.signature.is_some()).count();
                        eprintln!(
                            "{done}/{count} programs, {failed} failing, {:.0?}",
                            begun.elapsed()
                        );
                    }
                }
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|o| o.seed);
    // Group by signature.
    let mut groups: BTreeMap<Signature, Vec<u64>> = BTreeMap::new();
    let mut classes: BTreeMap<String, usize> = BTreeMap::new();
    let mut ok = 0;
    for o in &results {
        match &o.signature {
            Some(s) => {
                groups.entry(s.clone()).or_default().push(o.seed);
                *classes.entry(s.class.clone()).or_default() += 1;
            }
            None => ok += 1,
        }
    }
    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "{} programs (seeds {start}..{end}) in {:.0?}; weights: {}",
        results.len(),
        begun.elapsed(),
        if options.weights.is_empty() {
            "default".to_owned()
        } else {
            options.weights.join(" ")
        }
    );
    let _ = writeln!(summary, "\n{ok:6} ok");
    for (class, n) in &classes {
        let _ = writeln!(summary, "{n:6} {class}");
    }
    let _ = writeln!(summary, "\nDistinct signatures (count, first seeds):");
    for (sig, seeds) in &groups {
        let first: Vec<String> = seeds.iter().take(5).map(u64::to_string).collect();
        let _ = writeln!(
            summary,
            "{:6} {sig}\n       seeds {}",
            seeds.len(),
            first.join(" ")
        );
    }
    print!("{summary}");
    std::fs::write(out.join("summary.txt"), &summary).map_err(|e| e.to_string())?;
    if reduce {
        let _ = writeln!(summary, "\nReductions:");
        for (sig, seeds) in &groups {
            if sig.class == "invalid-program" {
                continue;
            }
            let seed = seeds[0];
            eprintln!("reducing seed {seed} ({sig})");
            match reduce_one(
                &tools,
                &cfg,
                Some(seed),
                None,
                out,
                &seed.to_string(),
                jobs,
                false,
            ) {
                Ok((text, s)) => {
                    let path = cases.join(seed.to_string()).join("reduced.rs");
                    let _ = std::fs::write(&path, text);
                    let line = format!("reduced {seed} ({s}): {}", path.display());
                    println!("{line}");
                    let _ = writeln!(summary, "{line}");
                }
                Err(e) => {
                    println!("reducing {seed} failed: {e}");
                    let _ = writeln!(summary, "reducing {seed} failed: {e}");
                }
            }
            // Keep the summary current: reductions take a while.
            let _ = std::fs::write(out.join("summary.txt"), &summary);
        }
    }
    Ok(())
}

fn save_case(
    cases: &Path,
    seed: u64,
    text: &str,
    v: &Verdict,
    sig: &Signature,
    options: &GenOptions,
) {
    let dir = cases.join(seed.to_string());
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("program.rs"), text);
    let weights: String = options
        .weights
        .iter()
        .map(|w| format!(" --weight {w}"))
        .collect();
    let info = format!(
        "seed {seed}\nsignature: {sig}\nreproduce: difftest test {seed}{weights}\nreduce: difftest reduce {seed}{weights}\n\n{}",
        driver::describe(v)
    );
    let _ = std::fs::write(dir.join("info.txt"), info);
}
