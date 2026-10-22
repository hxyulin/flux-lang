//! References in constants and statics: `&'static` values evaluated at
//! compile time and laid out in static memory, slices of structs looked
//! up by a function that returns a reference into one, references to
//! arrays, and temporaries in a static's value, which live forever.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

struct Entry {
    key: i32,
    value: i64,
}

const ONE: &i64 = &1;
const PRIMES: &[i64] = &[2, 3, 5, 7, 11];
static TABLE: &[Entry] = &[
    Entry { key: 1, value: 10 },
    Entry { key: 2, value: 20 },
    Entry { key: 3, value: 30 },
];
static BYTES: &[u8; 3] = &[104, 105, 33];
static COMPUTED: &i64 = &square(9);

const fn square(x: i64) -> i64 {
    x * x
}

fn lookup(key: i32) -> Option<&'static i64> {
    for e in TABLE {
        if e.key == key {
            return Some(&e.value);
        }
    }
    None
}

fn main() {
    show(1, *ONE);
    let mut total = 0;
    for p in PRIMES {
        total += *p;
    }
    show(2, total);
    show(3, PRIMES.len() as i64);
    show(4, *lookup(2).unwrap_or(&0));
    show(5, *lookup(7).unwrap_or(&-1));
    show(6, BYTES[1] as i64);
    show(7, *COMPUTED);
}
