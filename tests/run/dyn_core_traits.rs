//! Trait objects of `core`'s traits whose provided methods only a
//! vtable calls: `&mut dyn fmt::Write` with `write_fmt` and
//! `write_char`, and `&mut dyn Iterator` with `nth`.

use core::fmt::Write;

struct Count {
    bytes: usize,
    last: u8,
}

impl Write for Count {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.bytes += s.len();
        if let Some(&b) = s.as_bytes().last() {
            self.last = b;
        }
        Ok(())
    }
}

fn fill(w: &mut dyn Write, n: i32) -> core::fmt::Result {
    write!(w, "{n:>6}|{:?}", (n, "x"))?;
    w.write_char('z')
}

fn third(it: &mut dyn Iterator<Item = u32>) -> Option<u32> {
    it.nth(2)
}

fn main() {
    let mut c = Count { bytes: 0, last: 0 };
    let ok = fill(&mut c, 42).is_ok();
    let mut r = 10..20u32;
    println!("{} {} {} {:?} {:?}", ok, c.bytes, c.last as char, third(&mut r), r.next());
}
