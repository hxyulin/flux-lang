//! Host integration shared by the driver and source-graph validation harnesses.
pub mod identifiers;
pub mod proc_macro;

pub mod builtins;
pub mod flib;

mod build;
mod cargo;
mod driver;
mod flib_build;
mod package;
mod proc_macro_native;

use driver::{BuildOptions, CrateType, report_all};
pub use driver::{compiler_main, package_main};
