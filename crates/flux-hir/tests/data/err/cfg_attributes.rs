#[cfg_attr(all(), unknown_attribute)]
fn unknown() {}
#[cfg_attr(all(), cfg_attr(all(), unknown_nested))]
fn nested() {}
#[cfg_attr(any(), inline =)]
fn malformed_attribute() {}
#[cfg_attr(not(), inline)]
fn malformed_predicate() {}
#[cfg_attr(all() inline)]
fn missing_comma() {}
#[cfg_attr(all(), , inline)]
fn missing_attribute() {}
#[cfg_attr(all(), cfg(not()))]
fn malformed_expanded_cfg() {}
#[macro_use(named)]
mod macros { macro_rules! secret { () => { 1 }; } }
mod private { macro_rules! hidden { () => { 1 }; } }
fn before() -> i32 { later!() }
#[macro_use]
mod later_macros { macro_rules! later { () => { 1 }; } }
fn main() { hidden!(); }
