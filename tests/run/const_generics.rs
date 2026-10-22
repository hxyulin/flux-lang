//! Const generics: structs, `impl` blocks, functions and traits with
//! const parameters of integer types, `bool` and `char`; arguments as
//! literals, constants, other parameters, `{ expr }` blocks and
//! inferred from arrays; parameters as values, array lengths and repeat
//! counts, in closures, associated constants and `const fn`s evaluated
//! at compile time; instances monomorphized per value.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const LIMIT: usize = 3;

struct Stack<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy, const N: usize> Stack<T, N> {
    fn new(fill: T) -> Stack<T, N> {
        Stack {
            items: [fill; N],
            len: 0,
        }
    }

    fn push(&mut self, value: T) -> bool {
        if self.len == N {
            return false;
        }
        self.items[self.len] = value;
        self.len += 1;
        true
    }

    fn capacity(&self) -> usize {
        N
    }
}

struct Grid<const W: usize, const H: usize> {
    cells: [[u8; W]; H],
}

impl<const W: usize, const H: usize> Grid<W, H> {
    const AREA: usize = W * H;

    fn new() -> Self {
        Grid { cells: [[0; W]; H] }
    }

    fn zeros(&self) -> usize {
        let mut n = 0;
        for row in &self.cells {
            for &c in row {
                if c == 0 {
                    n += 1;
                }
            }
        }
        n
    }
}

struct Ring<T, const N: usize> {
    slots: [Option<T>; N],
    head: usize,
}

impl<T, const N: usize> Ring<T, N> {
    fn put(&mut self, value: T) -> Option<T> {
        let old = self.slots[self.head].take();
        self.slots[self.head] = Some(value);
        self.head = (self.head + 1) % N;
        old
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe { printf(c"drop %d\n".as_ptr(), self.0) };
    }
}

trait Summed {
    fn summed(&self) -> i64;
}

impl<const N: usize> Summed for [i64; N] {
    fn summed(&self) -> i64 {
        let mut s = 0;
        for x in self {
            s += *x;
        }
        s
    }
}

trait Width {
    const WIDTH: usize;
}

impl<T, const N: usize> Width for [T; N] {
    const WIDTH: usize = N;
}

trait Pick<const I: usize> {
    fn pick(&self) -> i32;
}

struct P;

impl Pick<0> for P {
    fn pick(&self) -> i32 {
        10
    }
}

impl Pick<1> for P {
    fn pick(&self) -> i32 {
        11
    }
}

const fn scaled<const N: usize>() -> usize {
    N * 10
}

const SCALED: usize = scaled::<4>();

fn sum<const N: usize>(a: [i32; N]) -> i32 {
    let mut s = 0;
    let mut i = 0;
    while i < N {
        s += a[i];
        i += 1;
    }
    s
}

fn twice<const N: usize>(a: [i32; N]) -> i32 {
    sum::<N>(a) + sum(a)
}

fn offset<const D: i32>(x: i32) -> i32 {
    x + D
}

fn code<const C: char>() -> u32 {
    C as u32
}

fn signed<const B: bool>(x: i32) -> i32 {
    if B { x } else { -x }
}

fn count<T, const N: usize>(a: [T; N]) -> usize {
    let mut n = 0;
    for _x in a {
        n += 1;
    }
    n
}

fn first_scaled<const N: usize>(a: [i32; N]) -> i32 {
    let f = |i: usize| a[i] * N as i32;
    f(0)
}

fn chunk<const N: usize>(values: &[i32]) -> [i32; N] {
    let mut out = [0; N];
    let mut i = 0;
    while i < N && i < values.len() {
        out[i] = values[i];
        i += 1;
    }
    out
}

fn width<A: Width>(_a: &A) -> usize {
    A::WIDTH
}

fn main() {
    let mut s = Stack::<i32, 4>::new(0);
    s.push(5);
    s.push(6);
    let t: Stack<u8, 2> = Stack::new(1);
    unsafe {
        printf(
            c"stack %d %d %d %d\n".as_ptr(),
            s.len as i32,
            s.items[1],
            s.capacity() as i32,
            t.capacity() as i32,
        )
    };

    let g: Grid<4, 2> = Grid::new();
    let h = Grid::<LIMIT, { LIMIT + 1 }>::new();
    unsafe {
        printf(
            c"grid %d %d %d\n".as_ptr(),
            Grid::<4, 2>::AREA as i32,
            g.zeros() as i32,
            h.zeros() as i32,
        )
    };

    unsafe {
        printf(
            c"fns %d %d %d %d %d %d %d %d\n".as_ptr(),
            sum([1, 2, 3]),
            twice([1, 1, 1, 1]),
            offset::<{ -5 }>(10),
            offset::<7>(1),
            code::<'a'>() as i32,
            signed::<false>(3) + signed::<true>(4),
            first_scaled([3, 4, 5]),
            offset::<{ LIMIT as i32 * 2 }>(0),
        )
    };

    let c: [i32; 3] = chunk(&[7, 8, 9, 10]);
    let a = [1i64, 2, 3];
    unsafe {
        printf(
            c"traits %d %lld %d %d %d %d %d\n".as_ptr(),
            c[2],
            a.summed(),
            width(&a) as i32,
            <[u8; 7] as Width>::WIDTH as i32,
            <P as Pick<0>>::pick(&P),
            <P as Pick<1>>::pick(&P),
            SCALED as i32 + scaled::<2>() as i32,
        )
    };

    let mut r: Ring<Noisy, 2> = Ring {
        slots: [None, None],
        head: 0,
    };
    r.put(Noisy(1));
    r.put(Noisy(2));
    let old = r.put(Noisy(3));
    let n = count([Noisy(4), Noisy(5)]);
    unsafe { printf(c"ring %d %d\n".as_ptr(), old.is_some() as i32, n as i32) };
}
