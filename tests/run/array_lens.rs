//! Array lengths and repeat counts written as constant expressions:
//! arithmetic on constants, calls of `const fn`s, blocks, and associated
//! constants through `Self` in an `impl` block without generic
//! parameters, evaluated at compile time like any constant.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const LIMIT: usize = 6;
const WIDTH: usize = 3;

const fn double(n: usize) -> usize {
    n * 2
}

struct Sieve {
    composite: [bool; LIMIT + 1],
}

struct Buffer;

impl Buffer {
    const CAP: usize = 2;

    fn fresh() -> [u8; Self::CAP * 2] {
        [7; Self::CAP + 2]
    }
}

fn grid(fill: i32) -> [[i32; WIDTH * 2]; { WIDTH - 1 }] {
    [[fill; WIDTH * 2]; WIDTH - 1]
}

fn sieve() -> Sieve {
    let mut s = Sieve {
        composite: [false; LIMIT + 1],
    };
    let mut i = 2;
    while i <= LIMIT {
        let mut j = i * i;
        while j <= LIMIT {
            s.composite[j] = true;
            j += i;
        }
        i += 1;
    }
    s
}

fn main() {
    let s = sieve();
    let mut i = 2;
    while i < s.composite.len() {
        if !s.composite[i] {
            unsafe { printf(c"prime %d\n".as_ptr(), i as i32) };
        }
        i += 1;
    }
    let doubled: [u16; double(WIDTH)] = [5; double(3)];
    let g = grid(4);
    let b = Buffer::fresh();
    let blocks = [1u8; {
        const EXTRA: usize = 1;
        LIMIT / 2 + EXTRA
    }];
    unsafe {
        printf(
            c"%d %d %d %d %d %d\n".as_ptr(),
            doubled.len() as i32,
            g.len() as i32,
            g[0].len() as i32,
            b.len() as i32,
            b[3] as i32,
            blocks.len() as i32,
        )
    };
}
