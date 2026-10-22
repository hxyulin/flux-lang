//! Miscompile (debug and -O): a `static` of an enum, one of whose
//! variants holds an array, is laid out in memory with another variant's
//! fields at different offsets than the code reading it uses.
//!
//! rustc prints `B 8 3` four times, then `err 7`. flux prints `B 3 0`
//! for the static `G`, and `B 8 3` for the constant `C`, the local and
//! the static `H` (whose enum has no array), then `err 0` for the static
//! `R`, a `Result<[i32; 2], i64>`, with and without `-O`.
//!
//! Found by difftest (seeds 532, 634 and 162 of the first campaign, and
//! 102374 and 104585 of the second, through `Result<S, i64>` statics
//! whose struct holds an array; 162 ends in SIGTRAP: a static `[E1; 3]`
//! whose garbled discriminant reaches an impossible `match` arm).
//!
//! Diagnosis: flux's lowering, not keel (`difftest bisect-opt` reports
//! the difference with no optimization passes). `difftest ir` shows the
//! static's data as `b"\x01\0\0\0\0\0\0\0\0\0\0\0\x08\0\0\0\x03\0\0\0..."`:
//! discriminant at offset 0, `8` at offset 12 and `3` at 16, while the
//! code building and reading an `E::B` stores and loads its fields at
//! offsets 16 and 20 (`H`'s data, without an array in the enum, has them
//! at 16 and 20 too). Likewise `R`'s data has `7` at offset 12, which
//! code reads at 16. So the code that writes a constant enum value into
//! a static's bytes and the layout codegen uses disagree on where a
//! variant's fields go when another variant holds an array (which makes
//! values containing it live in memory rather than in registers).

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

enum E {
    A([i32; 2]),
    B(i32, i32),
}

enum F {
    A(i64),
    B(i32, i32),
}

static G: E = E::B(8, 3);
static H: F = F::B(8, 3);
static R: Result<[i32; 2], i64> = Err(7);
const C: E = E::B(8, 3);

fn show(e: &E) {
    match e {
        E::A(a) => unsafe { printf(c"A %d %d\n".as_ptr(), a[0], a[1]) },
        E::B(a, b) => unsafe { printf(c"B %d %d\n".as_ptr(), *a, *b) },
    };
}

fn main() {
    show(&G);
    show(&C);
    let l = E::B(8, 3);
    show(&l);
    match &H {
        F::A(a) => unsafe { printf(c"A %lld\n".as_ptr(), *a) },
        F::B(a, b) => unsafe { printf(c"B %d %d\n".as_ptr(), *a, *b) },
    };
    match &R {
        Ok(a) => unsafe { printf(c"ok %d %d\n".as_ptr(), a[0], a[1]) },
        Err(e) => unsafe { printf(c"err %lld\n".as_ptr(), *e) },
    };
}
