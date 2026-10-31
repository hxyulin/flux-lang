//! Bounded compiler queries and compile probes for native build scripts.
use clap::Parser;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "fluxc compiler probe")]
struct Probe {
    source: PathBuf,
    #[arg(long, default_value = "2024")]
    edition: String,
    #[arg(long)]
    crate_name: Option<String>,
    #[arg(long, default_value = "lib")]
    crate_type: String,
    #[arg(long, default_value = "metadata")]
    emit: String,
    #[arg(long)]
    out_dir: PathBuf,
    #[arg(long)]
    target: Option<String>,
    #[arg(long = "cfg")]
    cfgs: Vec<String>,
    #[arg(long)]
    cap_lints: Option<String>,
}

pub(crate) fn run() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--version" {
        println!("fluxc {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() == 2 && args[0] == "--print" && args[1] == "cfg" {
        return finish(crate::target::cfg().map(|cfg| {
            for line in cfg {
                println!("{line}");
            }
        }));
    }
    finish(compile(Probe::parse()))
}

fn finish(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn compile(probe: Probe) -> Result<(), String> {
    if !probe.emit.split(',').any(|kind| kind == "metadata")
        || probe.crate_type != "lib"
        || probe
            .emit
            .split(',')
            .any(|kind| !matches!(kind, "metadata" | "dep-info"))
    {
        return Err("only library metadata compiler probes are supported".into());
    }
    if probe
        .target
        .as_deref()
        .is_some_and(|target| crate::target::host() != Ok(target))
    {
        return Err("cross-target compiler probes are not supported".into());
    }
    if probe
        .cap_lints
        .as_deref()
        .is_some_and(|level| !matches!(level, "allow" | "warn"))
    {
        return Err("only allow/warn cap-lints are supported for compiler probes".into());
    }
    crate::builtins::initialize()?;
    std::fs::create_dir_all(&probe.out_dir).map_err(|e| e.to_string())?;
    let directory = probe.out_dir.canonicalize().map_err(|e| e.to_string())?;
    let runtime = directory.join("flux-probe-api");
    std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    flux_proc_macro::write_sources(&runtime).map_err(|e| e.to_string())?;
    let api = runtime.join("proc_macro.flib");
    let options = crate::BuildOptions {
        optimize: false,
        timings: false,
    };
    if !crate::flib_build::library(
        &runtime.join("lib.rs"),
        &api,
        "proc_macro",
        options,
        true,
        &[],
    )? {
        return Err("cannot compile native probe token API".into());
    }
    let name = probe
        .crate_name
        .unwrap_or_else(|| crate::build::crate_name(&probe.source));
    let source = crate::build::SourceInput {
        edition: crate::cargo::edition(&probe.edition)?,
        cfg: crate::cargo::cfg(&probe.cfgs, &[])?,
        ..crate::build::SourceInput::standalone(&probe.source, &name)
    };
    if !crate::flib_build::compile(
        &source,
        &directory.join("probe.flib"),
        options,
        crate::CrateType::Lib,
        true,
        &[("proc_macro".into(), api)],
    )? {
        return Err("compiler probe failed".into());
    }
    Ok(())
}
