//! Optimizer scheduling and opt-in observations with replayable input.
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use keel_ir::Module;
use keel_opt::{OptimizationReport, OptimizationTier, Pipeline};

use crate::{Error, Stage};

pub(crate) fn optimize(module: &mut Module, tier: OptimizationTier) -> Result<(), Error> {
    let Some(directory) = std::env::var_os("FLUX_KEEL_PROFILE") else {
        return keel_opt::run_passes(module, &passes(tier)).map_err(Error::optimize);
    };
    record(module, tier, Path::new(&directory))
}

fn passes(tier: OptimizationTier) -> Vec<&'static str> {
    let standard = tier.pipeline();
    if keel_opt::find_pass("compact-unused-functions").is_none() {
        return standard.to_vec();
    }
    let mut passes = Vec::with_capacity(standard.len() + 3);
    for &pass in standard {
        passes.push(if pass == "unused-functions" {
            "compact-unused-functions"
        } else {
            pass
        });
        if tier == OptimizationTier::Full && pass == "inline-cheap-reserve" {
            passes.push("compact-unused-functions");
        }
    }
    passes
}

fn record(module: &mut Module, tier: OptimizationTier, directory: &Path) -> Result<(), Error> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let io = |error| Error::io(Stage::Write, "cannot write keel optimizer profile", error);
    std::fs::create_dir_all(directory).map_err(io)?;
    let stem = directory.join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut input =
        BufWriter::new(std::fs::File::create(stem.with_extension("input.kl")).map_err(io)?);
    write!(input, "{module}").map_err(io)?;
    input.flush().map_err(io)?;
    let mut report = OptimizationReport::default();
    let result = Pipeline::named(&passes(tier))
        .map_err(Error::optimize)?
        .report(&mut report)
        .run(module);
    std::fs::write(stem.with_extension("json"), report.to_json()).map_err(io)?;
    result.map_err(Error::optimize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compacting_cleanup_removes_inlined_bodies_before_the_next_round() {
        if keel_opt::find_pass("compact-unused-functions").is_none() {
            eprintln!("skipped: backend does not expose compacting cleanup");
            return;
        }
        let mut module = keel_ir::parse("func @helper(i64) -> i64 cc(fast) local {\nblock0(v0: i64):\n v1 = iconst.i64 1\n v2 = iadd v0, v1\n return v2\n}\nfunc @live(i64) -> i64 {\nblock0(v0: i64):\n v1 = call @helper(v0)\n return v1\n}").unwrap();
        let mut report = OptimizationReport::default();
        Pipeline::named(&passes(OptimizationTier::Full))
            .unwrap()
            .report(&mut report)
            .run(&mut module)
            .unwrap();
        keel_ir::verify_module(&module).unwrap();
        let mut rounds = report
            .invocations
            .iter()
            .filter(|pass| pass.pass == "inline-cheap-reserve" && pass.function.is_none());
        assert_eq!(rounds.next().unwrap().before.functions, 2);
        assert_eq!(
            rounds.next().unwrap().before.functions,
            1,
            "an inlined private body must not reach the next inlining round"
        );
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn observations_preserve_optimized_ir_and_save_replayable_input() {
        let mut module = keel_ir::parse(
            "func @probe() -> i64 {\nblock0:\n v0 = iconst.i64 7\n v1 = iconst.i64 0\n v2 = iadd v0, v1\n return v2\n}\n",
        )
        .unwrap();
        let original = module.to_string();
        let mut expected = module.clone();
        keel_opt::run_passes(&mut expected, &passes(OptimizationTier::Full)).unwrap();
        let directory =
            std::env::temp_dir().join(format!("flux-keel-profile-{}", std::process::id()));
        record(&mut module, OptimizationTier::Full, &directory).unwrap();
        assert_eq!(module.to_string(), expected.to_string());
        let files: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(files.len(), 2);
        let input = files
            .iter()
            .find(|path| path.extension().unwrap() == "kl")
            .unwrap();
        let replay = std::fs::read_to_string(input).unwrap();
        assert_eq!(replay, original);
        keel_ir::verify_module(&keel_ir::parse(&replay).unwrap()).unwrap();
        let report = files
            .iter()
            .find(|path| path.extension().unwrap() == "json")
            .unwrap();
        assert!(
            std::fs::read_to_string(report)
                .unwrap()
                .contains("\"pass\"")
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
