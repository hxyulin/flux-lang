//! `format!`, `write!` into a `String` (`fmt::Write`), and `to_string`
//! of `Display` values, with the capacities rustc's give: `format!`'s
//! estimate from its literal text (literals that a plain `{}` shows are
//! written into the string, as rustc does), and `to_string`'s exact
//! room for text and the most digits of an integer. `Debug` and
//! `Display` of `Vec`, `String` and `Box`.

use core::fmt::Write;

struct Point {
    x: i32,
    y: i32,
}

impl core::fmt::Display for Point {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

fn main() {
    let a = format!("x = {}, y = {:?}", 5, "q");
    println!("{a} {}", a.capacity());
    let b = format!("{}", 42);
    println!("{b} {}", b.capacity());
    let n = 7;
    let c = format!("{n}{n:>4}|{:<3}|", "l");
    println!("{c} {}", c.capacity());
    let d = format!("a long piece of literal text with {} inside", n);
    println!("{d} {}", d.capacity());
    let e = format!("literal");
    println!("{e} {}", e.capacity());
    let mut out = String::new();
    write!(out, "{}-{}", 1, n).unwrap();
    writeln!(out, "!").unwrap();
    out.write_char('z').unwrap();
    println!("{out:?} {}", out.capacity());
    for t in [
        5.to_string(),
        (-17i64).to_string(),
        255u8.to_string(),
        u128::MAX.to_string(),
        true.to_string(),
        'é'.to_string(),
        "str".to_string(),
        String::from("own").to_string(),
        (&"r").to_string(),
        Point { x: 1, y: -2 }.to_string(),
    ] {
        println!("{t} {}", t.capacity());
    }
    let v = vec![String::from("a"), String::from("b\"c")];
    println!("{v:?} {:?} {:#?}", Box::new(3), vec![1, 2]);
    println!("[{:>6}] [{:<4}] {}", String::from("pad"), Box::new("bx"), Box::new(Point { x: 3, y: 4 }));
}
