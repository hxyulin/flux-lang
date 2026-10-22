#![allow(dead_code, unexpected_cfgs)]

#[cfg(test)]
mod tests {
    use missing_crate::NeverResolved;
    #[test]
    fn unavailable() { missing_macro!(); }
}

#[cfg(any())]
mod missing_file;
#[cfg(feature = "disabled")]
use missing_crate::Thing;
#[cfg(unknown_flux_option)]
extern crate missing_crate;
#[cfg(test)]
struct Hidden { value: missing_type!() }
#[cfg(test)]
enum HiddenEnum { A = missing_value() }
#[cfg(test)]
trait HiddenTrait: MissingTrait {}
#[cfg(test)]
impl MissingType { fn broken() {} }
#[cfg(test)]
type HiddenAlias = missing_type!();
#[cfg(test)]
const HIDDEN: [u8; missing_value()] = [];
#[cfg(test)]
static HIDDEN_STATIC: MissingType = missing_value();
#[cfg(test)]
unsafe extern "C" { fn missing(value: missing_type!()); }
unsafe extern "C" {
    #[cfg(test)]
    fn inactive(value: missing_type!());
}
#[cfg(test)]
compile_error!("inactive macro call");
#[cfg(test)]
macro_rules! chosen { () => { compile_error!("inactive definition") }; }

macro_rules! chosen { () => { 7 }; }
macro_rules! configured_item {
    () => {
        #[cfg(test)]
        fn generated() -> missing_type!() { missing_value() }
        #[cfg(all())]
        fn generated() -> i32 { 11 }
    };
}
configured_item!();

#[cfg(all())]
#[cfg(not(any(test, feature = "disabled", unknown_flux_option)))]
fn value() -> i32 { chosen!() }
#[cfg(all())]
#[cfg(test)]
fn value() -> i32 { missing_macro!() }

struct S;
impl S {
    #[cfg(test)]
    fn value() -> missing_type!() { missing_value() }
    #[cfg(all())]
    fn value() -> i32 { 13 }
    #[cfg(test)]
    const VALUE: [u8; missing_value()] = [];
    #[cfg(all())]
    const VALUE: i32 = 17;
}

trait Configured {
    #[cfg(test)]
    fn method() -> missing_type!();
    #[cfg(all())]
    fn method() -> i32;
    #[cfg(test)]
    const VALUE: MissingType;
    #[cfg(all())]
    const VALUE: i32;
    #[cfg(test)]
    type Output: MissingTrait;
    #[cfg(all())]
    type Output;
}
impl Configured for S {
    #[cfg(test)]
    fn method() -> missing_type!() { missing_value() }
    #[cfg(all())]
    fn method() -> i32 { 19 }
    #[cfg(test)]
    const VALUE: MissingType = missing_value();
    #[cfg(all())]
    const VALUE: i32 = 23;
    #[cfg(test)]
    type Output = missing_type!();
    #[cfg(all())]
    type Output = i32;
}

#[cfg(target_pointer_width = "64")]
fn width() -> i32 { 64 }
#[cfg(not(target_pointer_width = "64"))]
fn width() -> i32 { 32 }
#[cfg(target_endian = "little")]
fn endian() -> i32 { 1 }
#[cfg(not(target_endian = "little"))]
fn endian() -> i32 { 2 }

#[cfg(all(any(unix, windows), not(target_os = "imaginary")))]
fn target() -> bool {
    cfg!(all(any(unix, windows), not(target_os = "imaginary")))
}

fn main() {
    #[cfg(test)]
    fn local() -> missing_type!() { missing_value() }
    #[cfg(all())]
    fn local() -> i32 { 29 }
    println!("{} {} {} {} {} {} {} {} {}", value(), generated(), S::value(),
        S::VALUE, <S as Configured>::method(), <S as Configured>::VALUE,
        local(), width(), endian());
    println!("{}", target());
}
