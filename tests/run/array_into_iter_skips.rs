//! Skipping elements of an array's `IntoIter` drops them as rustc's
//! does: from the back (`nth_back`, and `rev().skip(n)` through `Rev`'s
//! `nth` and `Skip`'s use of it) as one slice, front to back; from the
//! front, in order.

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let n = [D(1), D(2), D(3)].into_iter().rev().skip(5).count();
    println!("rev skip past the end: {n}");
    let s: i32 = [D(4), D(5), D(6), D(7)].into_iter().rev().skip(2).map(|d| d.0).sum();
    println!("rev skip 2: {s}");
    let mut it = [D(8), D(9), D(10)].into_iter();
    let x = it.nth_back(1).map(|d| d.0);
    println!("nth_back(1): {x:?}");
    drop(it);
    let mut it = [D(11), D(12), D(13)].into_iter();
    println!("nth(1): {:?}", it.nth(1).map(|d| d.0));
    let n = [D(14), D(15), D(16)].into_iter().skip(5).count();
    println!("skip past the end: {n}");
    let mut it = [D(17), D(18), D(19), D(20)].into_iter().rev();
    println!("rev nth_back(1): {:?}", it.nth_back(1).map(|d| d.0));
}
