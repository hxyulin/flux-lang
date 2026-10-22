// A closure calling a method through a captured `&mut` borrows what it
// points to, and one calling `len` borrows the array: neither moves it,
// so the errors are of borrows, as in rustc.

struct S {
    v: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.v
    }
}

fn main() {
    let mut s = S { v: 1 };
    let m = &mut s;
    let f = || m.get();
    m.v = 2;
    f();
    let a = [S { v: 1 }];
    let g = || a.len();
    let b = a;
    g();
    let _ = b;
}
