// Slice methods' bounds, reported where rustc reports them.

struct S(i32);

fn main() {
    let mut a = [S(2), S(1)];
    a.sort();
    let mut b = [S(1)];
    b.copy_from_slice(&[S(2)]);
    let c = [S(1), S(2)];
    let d = c.contains(&S(1));
    let e = [1, 2, 3];
    let f = e.binary_search(1);
}
