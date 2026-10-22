//! Constant enum values in memory, where a variant's fields sit past
//! another variant's array: statics, constants and promoted constants of
//! enums with array-holding variants, `Result<[i32; 2], i64>`, `Option`
//! of a struct holding an array, nested enums, arrays and structs of such
//! enums, references to them in statics, and `static mut` ones written
//! and read back. Each value is read by code, which must find its fields
//! where the static's data put them.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Clone, Copy)]
enum E {
    A([i32; 2]),
    B(i32, i32),
}

#[derive(Clone, Copy)]
enum F {
    A(i64),
    B(i32, i32),
}

#[derive(Clone, Copy)]
struct Grid {
    tag: u8,
    cells: [u16; 3],
    last: i64,
}

#[derive(Clone, Copy)]
enum Outer {
    Empty,
    Bytes([u8; 5], u8),
    Inner(E, i32),
    Pair(Option<Grid>, i16),
}

#[derive(Clone, Copy)]
enum Wide {
    First([i64; 2]),
    Second(u8, [u8; 3], i64),
    Third(bool, char, u32),
}

#[repr(i8)]
#[derive(Clone, Copy)]
enum Signed {
    Neg([i8; 3]) = -2,
    Pos(i8, i16) = 5,
}

struct Holder {
    first: E,
    second: [E; 2],
    third: Result<[i32; 2], i64>,
}

static G: E = E::B(8, 3);
static GA: E = E::A([5, 6]);
static H: F = F::B(8, 3);
static HA: F = F::A(-5);
static R: Result<[i32; 2], i64> = Err(7);
static RO: Result<[i32; 2], i64> = Ok([-1, 2]);
static RS: Result<Grid, i64> = Err(-9);
static OG: Option<Grid> = Some(Grid {
    tag: 4,
    cells: [10, 20, 30],
    last: -40,
});
static ON: Option<Grid> = None;
static ARR: [E; 3] = [E::B(1, 2), E::A([3, 4]), E::B(5, 6)];
static OUTERS: [Outer; 5] = [
    Outer::Empty,
    Outer::Bytes([1, 2, 3, 4, 5], 6),
    Outer::Inner(E::B(70, 71), 72),
    Outer::Inner(E::A([80, 81]), 82),
    Outer::Pair(
        Some(Grid {
            tag: 9,
            cells: [91, 92, 93],
            last: 94,
        }),
        -95,
    ),
];
static WIDE: [Wide; 3] = [
    Wide::First([100, 101]),
    Wide::Second(102, [103, 104, 105], 106),
    Wide::Third(true, 'z', 107),
];
static SIGNED: [Signed; 2] = [Signed::Neg([-1, -2, -3]), Signed::Pos(-4, -500)];
static HOLDER: Holder = Holder {
    first: E::B(11, 12),
    second: [E::A([13, 14]), E::B(15, 16)],
    third: Err(17),
};
static TUPLE: (u8, E, Option<E>) = (1, E::B(2, 3), Some(E::B(4, 5)));
static REF: &E = &E::B(21, 22);
static REFS: &[Result<[i32; 2], i64>] = &[Err(23), Ok([24, 25]), Err(26)];
static mut MUT: E = E::B(31, 32);
static mut MUT_ARR: [Result<[i32; 2], i64>; 2] = [Err(33), Ok([34, 35])];

const C: E = E::B(8, 3);
const CR: Result<[i32; 2], i64> = Err(41);
const CARR: [Outer; 2] = [Outer::Inner(E::B(42, 43), 44), Outer::Bytes([0; 5], 45)];

fn show(label: i32, e: &E) {
    match e {
        E::A(a) => unsafe { printf(c"%d: A %d %d\n".as_ptr(), label, a[0], a[1]) },
        E::B(a, b) => unsafe { printf(c"%d: B %d %d\n".as_ptr(), label, *a, *b) },
    };
}

fn show_result(label: i32, r: &Result<[i32; 2], i64>) {
    match r {
        Ok(a) => unsafe { printf(c"%d: ok %d %d\n".as_ptr(), label, a[0], a[1]) },
        Err(e) => unsafe { printf(c"%d: err %lld\n".as_ptr(), label, *e) },
    };
}

fn show_grid(label: i32, g: &Option<Grid>) {
    match g {
        Some(g) => unsafe {
            printf(
                c"%d: grid %d %d %d %d %lld\n".as_ptr(),
                label,
                g.tag as i32,
                g.cells[0] as i32,
                g.cells[1] as i32,
                g.cells[2] as i32,
                g.last,
            )
        },
        None => unsafe { printf(c"%d: no grid\n".as_ptr(), label) },
    };
}

fn show_outer(label: i32, o: &Outer) {
    match o {
        Outer::Empty => unsafe { printf(c"%d: empty\n".as_ptr(), label) },
        Outer::Bytes(b, n) => unsafe {
            printf(
                c"%d: bytes %d %d %d\n".as_ptr(),
                label,
                b[0] as i32,
                b[4] as i32,
                *n as i32,
            )
        },
        Outer::Inner(e, n) => {
            show(label, e);
            unsafe { printf(c"%d: inner %d\n".as_ptr(), label, *n) }
        }
        Outer::Pair(g, n) => {
            show_grid(label, g);
            unsafe { printf(c"%d: pair %d\n".as_ptr(), label, *n as i32) }
        }
    };
}

fn show_wide(label: i32, w: &Wide) {
    match w {
        Wide::First(a) => unsafe { printf(c"%d: first %lld %lld\n".as_ptr(), label, a[0], a[1]) },
        Wide::Second(a, b, c) => unsafe {
            printf(
                c"%d: second %d %d %d %d %lld\n".as_ptr(),
                label,
                *a as i32,
                b[0] as i32,
                b[1] as i32,
                b[2] as i32,
                *c,
            )
        },
        Wide::Third(a, b, c) => unsafe {
            printf(c"%d: third %d %d %u\n".as_ptr(), label, *a as i32, *b as u32, *c)
        },
    };
}

fn show_signed(label: i32, s: &Signed) {
    match s {
        Signed::Neg(a) => unsafe {
            printf(
                c"%d: neg %d %d %d\n".as_ptr(),
                label,
                a[0] as i32,
                a[1] as i32,
                a[2] as i32,
            )
        },
        Signed::Pos(a, b) => unsafe {
            printf(c"%d: pos %d %d\n".as_ptr(), label, *a as i32, *b as i32)
        },
    };
}

fn main() {
    show(1, &G);
    show(2, &GA);
    show(3, &C);
    let local = E::B(8, 3);
    show(4, &local);
    for f in [&H, &HA] {
        match f {
            F::A(a) => unsafe { printf(c"5: A %lld\n".as_ptr(), *a) },
            F::B(a, b) => unsafe { printf(c"5: B %d %d\n".as_ptr(), *a, *b) },
        };
    }
    show_result(6, &R);
    show_result(7, &RO);
    match &RS {
        Ok(g) => show_grid(8, &Some(*g)),
        Err(e) => unsafe {
            printf(c"8: err %lld\n".as_ptr(), *e);
        },
    };
    show_grid(9, &OG);
    show_grid(10, &ON);
    for (i, e) in ARR.iter().enumerate() {
        show(11 + i as i32, e);
    }
    for (i, o) in OUTERS.iter().enumerate() {
        show_outer(20 + i as i32, o);
    }
    for (i, w) in WIDE.iter().enumerate() {
        show_wide(30 + i as i32, w);
    }
    for (i, s) in SIGNED.iter().enumerate() {
        show_signed(35 + i as i32, s);
        unsafe { printf(c"%d: tag %d\n".as_ptr(), 35 + i as i32, *(s as *const Signed as *const i8) as i32) };
    }
    show(40, &HOLDER.first);
    show(41, &HOLDER.second[0]);
    show(42, &HOLDER.second[1]);
    show_result(43, &HOLDER.third);
    unsafe { printf(c"44: %d\n".as_ptr(), TUPLE.0 as i32) };
    show(45, &TUPLE.1);
    if let Some(e) = &TUPLE.2 {
        show(46, e);
    }
    show(47, REF);
    for (i, r) in REFS.iter().enumerate() {
        show_result(50 + i as i32, r);
    }
    show_result(55, &CR);
    let copied = CR;
    show_result(56, &copied);
    for (i, o) in CARR.iter().enumerate() {
        show_outer(57 + i as i32, o);
    }
    // Promoted constants: references to constant expressions.
    let promoted: &E = &E::B(61, 62);
    show(60, promoted);
    let promoted_result: &Result<[i32; 2], i64> = &Err(63);
    show_result(61, promoted_result);
    let promoted_outer: &[Outer] = &[Outer::Inner(E::B(64, 65), 66), Outer::Empty];
    for (i, o) in promoted_outer.iter().enumerate() {
        show_outer(62 + i as i32, o);
    }
    unsafe {
        let m = MUT;
        show(70, &m);
        MUT = E::A([71, 72]);
        let m = MUT;
        show(71, &m);
        MUT = E::B(73, 74);
        let m = MUT;
        show(72, &m);
        let r = MUT_ARR[0];
        show_result(73, &r);
        let r = MUT_ARR[1];
        show_result(74, &r);
        MUT_ARR[0] = Ok([75, 76]);
        MUT_ARR[1] = Err(77);
        let r = MUT_ARR;
        show_result(75, &r[0]);
        show_result(76, &r[1]);
    }
    // Values copied out of statics into locals and compared.
    let g = G;
    let same = matches!((g, ARR[2]), (E::B(8, 3), E::B(5, 6)));
    unsafe { printf(c"80: %d\n".as_ptr(), same as i32) };
    let total: i64 = REFS
        .iter()
        .map(|r| match r {
            Ok(a) => (a[0] + a[1]) as i64,
            Err(e) => *e,
        })
        .sum();
    unsafe { printf(c"81: %lld\n".as_ptr(), total) };
}
