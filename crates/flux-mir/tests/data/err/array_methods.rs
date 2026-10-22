// A slice method's borrow of an array borrows the array.

fn main() {
    let mut a = [1, 2, 3];
    let first = a.first();
    a[0] = 5;
    let _ = first;
    let mut b = [1, 2];
    let iter = b.iter();
    b = [3, 4];
    let _ = iter;
}
