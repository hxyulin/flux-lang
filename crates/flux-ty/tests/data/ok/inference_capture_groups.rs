struct Pair(i32, i32);

fn main() {
    let mut a = Pair(1, 2);
    let mut b = Pair(3, 4);
    let c = 5;
    let mut capture = || {
        let _ = &b.1;
        let _ = &a.1;
        let _ = &b.0;
        let _ = &c;
        a.0 = c;
        b.1 = a.1;
        let _ = &a;
    };
    capture();
}
