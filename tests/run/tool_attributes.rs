//! Attributes in the namespaces of tools rustc registers (`clippy`,
//! `rustfmt`, `miri`, `rust_analyzer`) take any input and have no effect
//! on compilation, in every position an outer attribute can appear.

#[clippy::has_significant_drop]
struct Guard {
    #[rustfmt::skip]
    value: u32,
}

impl Drop for Guard {
    #[miri::any_name(with, [nested] {tokens})]
    fn drop(&mut self) {
        std::process::exit(self.value as i32 - 42 + 7);
    }
}

#[rust_analyzer::completions(ignore_flyimport)]
trait Doubler {
    #[clippy::format_args]
    fn double(&self) -> u32;
}

#[rustfmt::skip::macros(vec)]
impl Doubler for u32 {
    #[clippy::msrv = "1.0"]
    fn double(&self) -> u32 {
        #[rustfmt::skip]
        let doubled = *self * 2;
        doubled
    }
}

macro_rules! with_tool_attr {
    ($name:ident) => {
        #[clippy::msrv = "1.88"]
        fn $name(#[rustfmt::skip] x: u32) -> u32 {
            x + 1
        }
    };
}

with_tool_attr!(succ);

#[clippy::cognitive_complexity = "100"]
fn main() {
    let value = match 20u32.double() {
        #[clippy::too_many_lines]
        n => succ(n) + 1,
    };
    let _guard = Guard { #[rustfmt::skip] value };
    println!("{value}");
}
