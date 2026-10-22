//! Differential testing of flux against rustc, in the spirit of Csmith,
//! YARPGen and RustSmith.
//!
//! - [`generator`] generates random, valid, deterministic, terminating Rust
//!   programs inside flux's subset, from a seed.
//! - [`driver`] compiles each with rustc and with flux (with and without
//!   `-O`), runs them and classifies what happened.
//! - [`reduce`] shrinks a failing program while it keeps failing the same
//!   way, by editing the choices it was generated from and then deleting
//!   lines.
//!
//! The `difftest` binary runs campaigns (`just difftest`); see
//! CONTRIBUTING.md.

pub mod bisect;
pub mod driver;
pub mod generator;
pub mod reduce;
pub mod rng;

pub use generator::{Config, generate};
pub use rng::Source;

/// The program for `seed` under `cfg`, and the choices it was made from.
pub fn program(seed: u64, cfg: &Config) -> (String, Vec<u32>) {
    let (text, src) = generate(Source::random(seed), cfg);
    (text, src.tape().to_vec())
}
