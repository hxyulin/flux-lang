// Slice methods borrow what they are called on as their signatures say:
// `split_at_mut`'s halves borrow the array, a `&mut` result keeps its
// borrow once the call has activated it, and rustc's two-phase borrows
// do not extend to an array unsized for a method (`a.swap(0, a.len() -
// 1)` is E0502), though they do to a slice.

fn main() {
    let mut a = [1, 2, 3, 4];
    let (l, r) = a.split_at_mut(2);
    l[0] = r[0];
    a[0] = 1;
    r[1] = 2;
    let mut b = [1, 2, 3];
    let first = b.first_mut().unwrap();
    let w = b.windows(2).count();
    *first = 4;
    let mut c = [3, 1, 2];
    let s = &c[..];
    c.sort();
    let _ = s.len();
    let mut d = [3, 1, 2];
    d.swap(0, d.len() - 1);
    let e: &mut [i32] = &mut d;
    e.swap(0, e.len() - 1);
}
