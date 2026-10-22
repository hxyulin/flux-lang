//! Regression for difftest 014: closure pattern captures.

struct S {
    a: u8,
    b: u8,
}

fn main() {
    let mut v = (1, [S { a: 1, b: 2 }]);
    let c = || match v.1[0] {
        S { a: ref x, b: _ } => *x,
    };
    v.0 = 9;
    println!("{} {}", c(), v.0);
    let w = [S { a: 3, b: 4 }];
    let d = || match w[0] {
        S { a: ref x, b: _ } => *x,
    };
    println!("{}", d());
    let mut a = [[S { a: 5, b: 6 }]];
    let i = 0;
    let j = 0;
    let mut update = || match a[i][j] {
        S { a: ref mut x, b: _ } => *x += 2,
    };
    update();
    update();
    println!("{} {}", a[0][0].a, a[0][0].b);
}
