//! `{:?}` of `char` and `str` escapes what rustc's `char::escape_debug`
//! escapes: unprintable characters (control and format characters,
//! unassigned code points, noncharacters, private use, separators) and
//! grapheme-extending ones, through rustc's Unicode tables. Every code
//! point is formatted; each block of 4096 prints a hash of the output and
//! how many were escaped.

use core::fmt::Write;

struct Hash(u64, u64);

impl Write for Hash {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100000001b3);
            self.1 += 1;
        }
        Ok(())
    }
}

fn main() {
    let mut block = 0u32;
    while block < 0x110 {
        let mut h = Hash(0xcbf29ce484222325, 0);
        let mut escaped = 0;
        let mut c = block << 12;
        while c < (block + 1) << 12 {
            if let Some(ch) = char::from_u32(c) {
                let before = h.1;
                let _ = write!(h, "{:?}", ch);
                // `'x'` is at most 6 bytes; an escape is longer.
                if h.1 - before > 6 {
                    escaped += 1;
                }
            }
            c += 1;
        }
        println!("{:05x}: {:016x} {}", block << 12, h.0, escaped);
        block += 1;
    }
    for c in ['\u{ffff}', '\u{fdd0}', '\u{378}', '\u{80001}', '\u{300}', '\u{ad}', '\u{3164}', '\u{ff9e}', 'é'] {
        println!("{:?}", c);
    }
    println!("{:?}", "a\u{ffff}b\u{301}c\u{200b}d\u{e000}\u{1f600}");
}
