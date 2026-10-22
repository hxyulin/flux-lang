// Tuples whose elements are coerced are built with the coerced types,
// in place or in temporaries; a `&mut` reborrowed as a `&` is stored as
// one; a function item's value is a constant of its type.

fn double(x: i32) -> i32 {
    x * 2
}

fn read(r: &i32) -> i32 {
    *r
}

pub fn tuples(a: &[i32; 3]) -> usize {
    let table: [(i32, &[i32]); 1] = [(10, a)];
    let opt: Option<(i32, &[i32])> = Some((1, a));
    let nested: ((i32, &[i32]), i32) = ((1, a), 2);
    table[0].1.len() + (nested.0).1.len() + opt.unwrap().1.len()
}

pub fn reborrows(r: &mut i32) -> i32 {
    read(r) + read(&r)
}

pub fn least_upper_bound(c: bool, a: &[i32; 3], s: &[i32]) -> usize {
    let x = if c { a } else { s };
    x.len()
}

pub fn items() -> i32 {
    let f = double;
    f(2)
}

fn main() {
    let mut n = 1;
    tuples(&[1, 2, 3]);
    reborrows(&mut n);
    least_upper_bound(true, &[1, 2, 3], &[4]);
    items();
}
