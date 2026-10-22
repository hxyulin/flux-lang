//! The generator's programs are valid Rust: rustc accepts a few fixed
//! seeds' programs (with the flags difftest compiles with). Campaigns
//! (`just difftest`) are too slow for `cargo test`; this only guards the
//! generator. Skipped where rustc is not on the path.

use std::process::Command;

use flux_difftest::generator::Config;

const SEEDS: [u64; 6] = [0, 1, 7, 42, 1234, 99_999];

#[test]
fn rustc_accepts_generated_programs() {
    if Command::new("rustc").arg("--version").output().is_err() {
        eprintln!("skipped: rustc is not available");
        return;
    }
    let dir = std::env::temp_dir().join(format!("flux-difftest-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = Config::default();
    let mut failures = Vec::new();
    for seed in SEEDS {
        let (text, _) = flux_difftest::program(seed, &cfg);
        let src = dir.join(format!("p{seed}.rs"));
        std::fs::write(&src, &text).unwrap();
        let output = Command::new("rustc")
            .args([
                "--edition",
                "2024",
                "-C",
                "overflow-checks=off",
                "-C",
                "panic=abort",
            ])
            // A full build: some lints (overflow) run only on the way to code.
            .arg("-o")
            .arg(dir.join(format!("p{seed}")))
            .arg(&src)
            .output()
            .unwrap();
        if !output.status.success() {
            failures.push(format!(
                "seed {seed}:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        failures.is_empty(),
        "rustc rejected generated programs:\n{}",
        failures.join("\n")
    );
}

#[test]
fn generation_is_deterministic() {
    let cfg = Config::default();
    for seed in SEEDS {
        let (a, tape) = flux_difftest::program(seed, &cfg);
        let (b, _) = flux_difftest::program(seed, &cfg);
        assert_eq!(a, b, "seed {seed}");
        // Replaying the recorded choices gives the same program.
        let (c, _) = flux_difftest::generate(flux_difftest::Source::replay(tape), &cfg);
        assert_eq!(a, c, "seed {seed} replayed");
    }
}
