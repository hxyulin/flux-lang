#![rustfmt::skip]
mod inner { #![clippy::msrv = "1.0"] }
#[unsafe(clippy::has_significant_drop)]
struct Unsafe;
#[clippy]
fn bare_tool() {}
#[::clippy::format_args]
fn rooted() {}
#[rustdoc::unknown]
fn not_a_tool() {}
#[clippy::msrv = "1.88"]
fn main() {}
