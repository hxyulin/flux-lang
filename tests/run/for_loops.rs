//! `for` loops and drop order: the iterator is dropped when the loop
//! ends, after the last item's bindings; items with destructors are
//! dropped at the end of each iteration, or as `break` and `continue`
//! leave it; labeled loops are left from inside nested ones; the head's
//! temporaries live until the loop ends; and loops in closures.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(100, self.0);
    }
}

/// Gives `Noisy` values counting down, and is noisy itself.
struct Countdown {
    n: i32,
    _guard: Noisy,
}

impl Iterator for Countdown {
    type Item = Noisy;
    fn next(&mut self) -> Option<Noisy> {
        if self.n == 0 {
            None
        } else {
            self.n -= 1;
            Some(Noisy(self.n))
        }
    }
}

fn countdown(n: i32, id: i32) -> Countdown {
    Countdown {
        n,
        _guard: Noisy(id),
    }
}

impl Noisy {
    fn get(&self) -> i32 {
        self.0
    }
}

fn make(id: i32) -> Noisy {
    Noisy(id)
}

fn main() {
    // Each item dropped at the end of its iteration, then the iterator.
    for item in countdown(3, 50) {
        show(1, item.0);
    }
    // `break` drops the item and the iterator.
    for item in countdown(5, 51) {
        if item.0 == 3 {
            break;
        }
        show(2, item.0);
    }
    // `continue` drops the item.
    for item in countdown(3, 52) {
        if item.0 == 1 {
            continue;
        }
        show(3, item.0);
    }
    // Items moved out are not dropped by the loop.
    let mut kept = Noisy(0);
    for item in countdown(2, 53) {
        kept = item;
    }
    show(4, kept.0);
    // The head's temporaries live until the loop ends.
    for i in 0..make(2).get() {
        show(5, i);
    }
    // Labeled loops.
    let mut found = 0;
    'rows: for row in 0..5 {
        for col in countdown(4, 60 + row) {
            if col.0 + row == 4 {
                found = found * 10 + row;
                continue 'rows;
            }
            if row == 3 {
                break 'rows;
            }
        }
    }
    show(6, found);
    // A `while` and `loop` with labels.
    let mut n = 0;
    'outer: while n < 100 {
        loop {
            n += 7;
            if n % 3 == 0 {
                continue 'outer;
            }
            if n > 50 {
                break 'outer;
            }
            break;
        }
    }
    show(7, n);
    // Loops in closures.
    let sum_to = |n: i32| {
        let mut total = 0;
        for i in 1..=n {
            total += i;
        }
        total
    };
    show(8, sum_to(10));
    // Ranges of other integer types, and empty ones.
    let mut bytes = 0u32;
    for b in 250u8..=255 {
        bytes += b as u32;
    }
    show(9, bytes as i32);
    for _ in 5..5 {
        show(10, 0);
    }
    for i in (0..10i64).rev().step_by(4) {
        show(11, i as i32);
    }
}
