struct Record { #[cfg(not())] field: i32 }
struct Tuple(#[cfg_attr(all(), cfg(not()))] i32);
enum Choice { #[cfg(not())] Variant }
fn function(#[cfg(not())] value: i32) {}
type Function = fn(#[cfg(not())] i32);
fn closure() { let closure = |#[cfg_attr(all(), cfg(not()))] value: i32| value; }
