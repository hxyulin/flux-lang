//! Constant expressions (array lengths, repeat counts and const generic
//! arguments) inside macro calls in function bodies, in expression and
//! statement position, in generic functions and methods too.

struct Ranged<const MIN: i32, const MAX: i32>(i32);

impl<const MIN: i32, const MAX: i32> Ranged<MIN, MAX> {
    fn new(value: i32) -> Option<Self> {
        if MIN <= value && value <= MAX { Some(Ranged(value)) } else { None }
    }
}

const fn width<const N: usize>() -> usize {
    N
}

macro_rules! zeros {
    ($n:expr) => {
        [0u8; $n * 2]
    };
}

macro_rules! declare {
    ($name:ident) => {
        let $name: [u16; 1 + 2] = [7; 3];
    };
}

macro_rules! ranged {
    ($value:expr) => {
        Ranged::<-9_999, { 9_000 + 999 }>::new($value)
    };
}

macro_rules! sized {
    () => {
        width::<{ 4 * 4 }>()
    };
}

struct Holder<T>(T);

impl<T: Copy> Holder<T> {
    fn spread(&self) -> [T; 2] {
        let values: [T; 1 + 1] = [self.0; 2];
        sized!();
        values
    }
}

fn generic<T: Default + Copy>() -> usize {
    let array: [T; 2 * 2] = [T::default(); 4];
    array.len() + zeros!(1).len() + sized!()
}

macro_rules! assert_constant {
    ($cond:expr) => {{
        let [()] = [(); (true & $cond) as usize];
    }};
}

macro_rules! terminated {
    ($text:expr) => {{
        const BYTES: &[u8] = concat!($text, "\0").as_bytes();
        const _: () = assert_constant!(BYTES.len() > 1);
        BYTES
    }};
}

fn main() {
    core::pin::pin!([3u8; 1 + 2]);
    assert_eq!(terminated!("hi"), b"hi\0");
    static WIDTH: usize = sized!();
    assert_eq!(WIDTH, 16);
    let a = zeros!(3);
    declare!(b);
    let r = ranged!(1234).map_or(-1, |r| r.0);
    let out = ranged!(10_000).map_or(-1, |r| r.0);
    let h = Holder(5u8).spread();
    println!("{} {} {} {} {} {} {}", a.len(), b[0] + b[2], r, out, sized!(), h[1], generic::<u32>());
    std::process::exit(a.len() as i32 + sized!() as i32);
}
