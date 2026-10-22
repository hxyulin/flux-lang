//! `write!` and `writeln!` into a `fmt::Write` of the program's own (a
//! fixed buffer), `Display` impls written with `write!`, `pad` and
//! `Formatter`'s options, and `fmt::Error` from a writer that is full.

use core::fmt::{self, Write};

struct Buf {
    bytes: [u8; 32],
    len: usize,
}

impl Buf {
    fn new() -> Buf {
        Buf {
            bytes: [0; 32],
            len: 0,
        }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
}

impl Write for Buf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if self.len + s.len() > self.bytes.len() {
            return Err(fmt::Error);
        }
        for b in s.bytes() {
            self.bytes[self.len] = b;
            self.len += 1;
        }
        Ok(())
    }
}

struct Celsius(i32);

impl fmt::Display for Celsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(p) = f.precision() {
            write!(f, "{:.*}°C!", p, "temperature")
        } else if f.alternate() {
            write!(f, "{}°", self.0)
        } else {
            write!(f, "{}°C", self.0)
        }
    }
}

struct Name(&'static str);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        println!(
            "fill {:?} align {:?} width {:?} precision {:?} plus {} minus {} zero {}",
            f.fill(),
            f.align(),
            f.width(),
            f.precision(),
            f.sign_plus(),
            f.sign_minus(),
            f.sign_aware_zero_pad()
        );
        f.pad(self.0)
    }
}

fn fill(n: i32) -> Result<Buf, fmt::Error> {
    let mut b = Buf::new();
    write!(b, "n={}", n)?;
    writeln!(b, ", hex={:#x}", n)?;
    b.write_char('!')?;
    writeln!(b)?;
    Ok(b)
}

fn main() {
    let b = fill(255).unwrap();
    print!("{}", b.as_str());
    let mut small = Buf::new();
    let r = write!(small, "{}", "this text is longer than thirty-two bytes");
    println!("{:?} {:?} {}", r, small.as_str(), small.len);
    let r = write!(small, "{}-{}", 1, "ok");
    println!("{:?} {:?}", r, small.as_str());
    println!("[{}] [{:#}] [{:.3}] [{:>12}]", Celsius(21), Celsius(-4), Celsius(0), Celsius(5));
    println!("[{}] [{:*^9}] [{:<6.2}] [{:+08}]", Name("ann"), Name("bob"), Name("carol"), Name("x"));
    let mut b = Buf::new();
    b.write_fmt(format_args!("{}+{}", 2, 3)).unwrap();
    println!("{} {:?}", b.as_str(), format_args!("{} {}", 'a', "b"));
    println!("{}", format_args!("nested {}", format_args!("{:03}", 7)));
}
