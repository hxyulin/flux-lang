//! The drop order of an array a slice pattern moved some elements out
//! of, on paths where it did and did not: as rustc's elaborated drops,
//! the runs of elements without moves of their own and those with, the
//! last run first.

struct Loud(i32);

impl Drop for Loud {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn e1(arr: [Loud; 4], k: bool) {
    if k {
        let [a, ..] = arr;
        let _ = a;
    }
    println!("e1 end");
}

fn e2(arr: [Loud; 4]) {
    let [_, b, ..] = arr;
    let _ = b;
    println!("e2 end");
}

fn e3(arr: [Loud; 4], k: bool) {
    if k {
        let [_, b, _, d] = arr;
        let _ = (b, d);
    }
    println!("e3 end");
}

fn e4(arr: [Loud; 4], k: bool) {
    if k {
        let [_, rest @ ..] = arr;
        let _ = rest;
    }
    println!("e4 end");
}

fn main() {
    e1([Loud(1), Loud(2), Loud(3), Loud(4)], false);
    e2([Loud(1), Loud(2), Loud(3), Loud(4)]);
    e3([Loud(1), Loud(2), Loud(3), Loud(4)], false);
    e3([Loud(1), Loud(2), Loud(3), Loud(4)], true);
    e4([Loud(1), Loud(2), Loud(3), Loud(4)], false);
    e4([Loud(1), Loud(2), Loud(3), Loud(4)], true);
}
