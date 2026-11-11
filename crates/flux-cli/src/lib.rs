//! Host integration shared by the driver and source-graph validation harnesses.
pub mod identifiers;
pub mod proc_macro;

pub mod builtins;
pub mod flib;

mod artifact_cache;
mod backend_units;
mod backend_workers;
mod build;
mod cargo;
mod doctest;
mod driver;
mod flib_build;
mod host_cache;
mod native_link;
mod package;
mod proc_macro_native;

use driver::{BuildOptions, CrateType, report_all};
pub use driver::{compiler_main, package_main};

mod target;

mod probe;
