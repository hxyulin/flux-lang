type A = X<Y<dyn Z + W>: V>;

fn f(s: S) {
    let _ = a == b && c != d;
    let _ = (a == b) == c;
    let _ = 1.. ..3;
    let _ = (1..2)..3;
    let _ = S { a: 1, ..s };
}
