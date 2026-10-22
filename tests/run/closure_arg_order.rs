//! Closures passed to methods are checked after the other arguments are
//! coerced to their parameters, as rustc checks them: what `fold`'s
//! `init` says of the accumulator is known in the closure, where
//! methods are called on it.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

struct Acc {
    total: i32,
}

impl Acc {
    fn push(self, x: i32) -> Acc {
        Acc {
            total: self.total * 2 + x,
        }
    }
}

fn main() {
    let x = [1, 2, 3];
    let r = x.iter().fold(0i32, |acc, &a| acc.wrapping_add(a));
    show(1, r);
    let pairs = [(1u8, 2u8), (3, 4)];
    let h = pairs
        .iter()
        .fold(7u32, |acc, &(a, b)| acc.wrapping_mul(31).wrapping_add(a as u32 * b as u32));
    show(2, h as i32);
    let acc = (1..4).fold(Acc { total: 0 }, |acc, x| acc.push(x));
    show(3, acc.total);
    let len = Some("abc").map_or(0usize, |s| s.len());
    show(4, len as i32);
    let m = x.iter().fold(i64::MIN, |best, &v| best.max(v as i64));
    show(5, m as i32);
}
