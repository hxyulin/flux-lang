// Borrows that operators on references make: a compound assignment
// with a reference on the right borrows its left side mutably, which
// needs `mut` (E0596), and `<` of a `&mut` reference with itself
// reborrows it mutably while it is borrowed (E0502), as in rustc.

fn compound_needs_mut(r: &i32) -> i32 {
    let x = 1;
    x += r;
    x
}

fn compare_with_itself(m: &mut i32) -> bool {
    m < m
}

fn main() {
    let mut m = 1;
    compound_needs_mut(&1);
    compare_with_itself(&mut m);
}
