// Operators on references in MIR: calls of `core`'s impls for
// references (`<&i32 as Add<i32>>::add`), which take the references as
// they are; `==` and `<` pass them to the referents' `eq` and `lt`,
// reborrowing a `&mut` as shared, so it stays usable, except that `<`
// of `&mut` references borrows the left one and reborrows the right one
// mutably, as rustc's `PartialOrd::lt(&a, &mut *b)` does; a compound
// assignment borrows its left side mutably.

fn arithmetic(r: &i32, x: i32) -> i32 {
    let a = r + x;
    let b = x * r;
    -r + a + b
}

fn shift(r: &u8, n: &u32) -> u8 {
    r << n
}

fn compound(r: &i64) -> i64 {
    let mut x = 1;
    x += r;
    x
}

fn compare(a: &i32, m: &mut i32, n: &mut i32) -> bool {
    let eq = a == m;
    *m += 1;
    eq || a < m || m <= n
}

fn main() {
    let (mut m, mut n) = (2, 3);
    arithmetic(&1, 2);
    shift(&1, &2);
    compound(&3);
    compare(&1, &mut m, &mut n);
}
