// Comparisons through `PartialOrd` in MIR: `<` on integers is built in,
// on other types a call of `PartialOrd::lt` with both operands borrowed,
// and on references a call of the referents' `lt` with the references
// as they are. A derived `partial_cmp` matches on each field's
// comparison in turn, and an enum's reads both discriminants first.

#[derive(PartialEq, PartialOrd)]
struct Pair(i32, u8);

#[derive(PartialEq, PartialOrd)]
enum Shape {
    Dot,
    Line(i32),
}

fn integers(a: i32, b: i32) -> bool {
    a < b
}

fn structs(p: Pair, q: Pair) -> bool {
    p <= q
}

fn references(a: &u8, b: &u8) -> bool {
    a > b
}

fn generic<T: Ord>(a: T, b: T) -> T {
    a.max(b)
}

fn main() {
    integers(1, 2);
    structs(Pair(1, 2), Pair(1, 3));
    references(&1, &2);
    generic(1, 2);
    let _ = Shape::Dot < Shape::Line(1);
}
