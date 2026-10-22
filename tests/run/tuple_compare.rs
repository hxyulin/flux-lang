//! Comparing tuples (`==`, `!=`, `<` and the others, `cmp`, `max`),
//! through references and in derives of structs with tuple and array
//! fields; arrays' `Eq`, `PartialOrd` and `Ord`; and the order in which
//! a tuple's comparison calls its elements' methods.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

use core::cmp::Ordering;

fn show(label: i32, value: i64) {
    unsafe {
        printf(c"%d: %lld\n".as_ptr(), label, value);
    }
}

fn ord(o: Ordering) -> i64 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// Reports each comparison it makes.
#[derive(Clone, Copy)]
struct Loud(i32);

impl PartialEq for Loud {
    fn eq(&self, other: &Loud) -> bool {
        show(500, self.0 as i64);
        self.0 == other.0
    }
    fn ne(&self, other: &Loud) -> bool {
        show(501, self.0 as i64);
        self.0 != other.0
    }
}

impl PartialOrd for Loud {
    fn partial_cmp(&self, other: &Loud) -> Option<Ordering> {
        show(502, self.0 as i64);
        self.0.partial_cmp(&other.0)
    }
    fn lt(&self, other: &Loud) -> bool {
        show(503, self.0 as i64);
        self.0 < other.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
struct Cell {
    pos: (u8, i16),
    grid: [[bool; 3]; 2],
    tag: [u8; 2],
}

fn lt<T: PartialOrd>(a: &T, b: &T) -> bool {
    a < b
}

fn same<T: Eq>(a: T, b: T) -> bool {
    a == b
}

fn main() {
    let a = (1, 2u8, true);
    let b = (1, 3u8, false);
    show(1, (a == b) as i64);
    show(2, (a != b) as i64);
    show(3, (a < b) as i64);
    show(4, (a >= b) as i64);
    show(5, ord(a.cmp(&b)));
    show(6, (&a == &a) as i64);
    show(7, (&a < &b) as i64);
    show(8, lt(&(5, -1), &(5, 0)) as i64);
    show(9, same((1,), (1,)) as i64);
    let m = (2, 'x').max((2, 'y'));
    show(10, m.1 as i64);
    show(11, ((1, (2, 3)) < (1, (2, 4))) as i64);
    let pairs: &[(i32, i32)] = &[(3, 1), (1, 9), (3, 0)];
    let best = pairs.iter().max().unwrap();
    show(12, (best.0 * 10 + best.1) as i64);
    let low = pairs.iter().copied().min().unwrap();
    show(13, (low.0 * 10 + low.1) as i64);
    let big = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
    let bigger = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13);
    show(14, (big < bigger) as i64);
    show(15, (big.partial_cmp(&bigger) == Some(Ordering::Less)) as i64);

    // Each element's method, in rustc's order.
    let x = (Loud(1), Loud(2));
    let y = (Loud(1), Loud(3));
    show(20, (x == y) as i64);
    show(21, (x != y) as i64);
    show(22, (x < y) as i64);
    show(23, (x.partial_cmp(&y) == Some(Ordering::Less)) as i64);
    show(24, (x > y) as i64);

    // Arrays.
    let p = [1, 2, 3];
    let q = [1, 3, 0];
    show(30, (p < q) as i64);
    show(31, ord(p.cmp(&q)));
    show(32, (p >= q) as i64);
    show(33, same([true, false], [true, false]) as i64);
    show(34, ([[1u8, 2], [3, 4]] > [[1, 2], [3, 3]]) as i64);
    let e: [i32; 0] = [];
    show(35, ord(e.cmp(&[])));
    let lp = [Loud(4), Loud(5)];
    let lq = [Loud(4), Loud(6)];
    show(36, (lp < lq) as i64);

    // Derives with tuple and array fields.
    let c1 = Cell {
        pos: (1, -2),
        grid: [[false; 3]; 2],
        tag: [1, 2],
    };
    let mut c2 = c1;
    show(40, (c1 == c2) as i64);
    c2.grid[1][2] = true;
    show(41, (c1 < c2) as i64);
    show(42, ord(c2.cmp(&c1)));
    c2.pos = (0, 100);
    show(43, (c1 > c2) as i64);
    let d = Cell::default();
    show(44, (d.pos == (0, 0) && d.tag == [0, 0]) as i64);
}
