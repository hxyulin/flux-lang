//! Format strings that a macro makes, such as `concat!(...)` or a
//! `macro_rules!` macro that expands to a literal, in `panic!`,
//! `unreachable!`, `assert!` messages and the `format_args!` family,
//! also when an `expr` fragment forwards the macro call.

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

macro_rules! sequence {
    (struct $name:ident => $value:expr) => {
        struct $name;

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, $value)
            }
        }
    };
}

sequence!(struct HideCursor => csi!("?25l"));
sequence!(struct Joined => concat!(csi!("?1000h"), csi!("?1006h")));

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
    write!(s, "{}{}", HideCursor, Joined).unwrap();
    print!("{}", s.len());
    println!(concat!(" ", "{}"), check(7));
    println!("{}", format!(concat!("{}", "{}"), 3, 4));
    check(45);
}
