// `cfg(true)` and `cfg(false)` are boolean literals, stable since Rust 1.88.
#[cfg(true)]
fn value() -> i32 { 1 }
#[cfg(false)]
fn value() -> i32 { missing!() }

#[cfg(all(true, not(false), any(false, true)))]
fn nested() -> i32 { 2 }
#[cfg(any(false, not(true)))]
fn nested() -> i32 { missing!() }

#[cfg_attr(true, derive(Clone, Copy))]
#[cfg_attr(false, derive(Missing))]
struct Pair(i32, i32);

fn main() {
    let pair = Pair(value(), nested());
    let copy = pair;
    println!("{} {}", pair.0 + copy.1, cfg!(true));
    println!("{} {}", cfg!(false), cfg!(all(true, not(false))));
    if cfg!(any(false, false)) {
        std::process::exit(1);
    }
}
