//! Format strings that a macro makes, such as `concat!(...)` or a
//! `macro_rules!` macro that expands to a literal, in `panic!`,
//! `unreachable!`, `assert!` messages and the `format_args!` family.

use std::fmt::Write as _;

macro_rules! csi {
    ($($l:expr),*) => { concat!("\x1B[", $($l),*) };
}

macro_rules! bug {
    ($descr:literal) => {
        panic!(concat!("internal error: ", $descr))
    };
}

macro_rules! pair_format {
    () => {
        "{}-{}"
    };
}

fn check(n: u32) -> u32 {
    if n > 100 {
        bug!("too big");
    }
    if n > 50 {
        unreachable!(concat!("over ", "fifty: {}"), n);
    }
    assert!(n < 40, concat!("assert {}", "!"), n);
    n
}

fn main() {
    let mut s = String::new();
    write!(s, csi!("?1049h")).unwrap();
    write!(s, concat!("<", "{}", ">"), 5).unwrap();
    writeln!(s, pair_format!(), 1, 2).unwrap();
    print!("{}", s.len());
    println!(concat!(" ", "{}"), check(7));
    println!("{}", format!(concat!("{}", "{}"), 3, 4));
    check(45);
}
