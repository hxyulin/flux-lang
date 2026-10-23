//! Labeled block exits, coercions and cleanup match rustc in both modes.
struct Noisy(i32);
impl Drop for Noisy {
    fn drop(&mut self) { println!("drop {}", self.0); }
}
fn choose(early: bool, n: i32) -> i32 {
    'alias: {
        if early { break 'alias n + 10; }
        n + 20
    }
}
fn nested(n: i32) -> i32 {
    'outer: {
        let _outer = Noisy(1);
        let mut i = 0;
        'scan: loop {
            let _iteration = Noisy(10 + i);
            i += 1;
            'inner: {
                let _inner = Noisy(20 + i);
                if i < n { continue 'scan; }
                if i == n { break 'outer i * 3; }
                break 'inner;
            }
            break;
        }
        99
    }
}
fn moved(early: bool) -> Noisy {
    'value: {
        let _first = Noisy(30);
        let value = Noisy(31);
        let _last = Noisy(32);
        if early { break 'value value; }
        value
    }
}
fn slice<'a>(early: bool, a: &'a [i32; 2], b: &'a [i32; 3]) -> &'a [i32] {
    'value: { if early { break 'value a; } b }
}
fn initialized(early: bool) -> i32 {
    let x;
    'init: {
        if early { x = 7; break 'init; }
        x = 8;
    }
    x
}
fn main() {
    println!("{} {}", choose(true, 2), choose(false, 2));
    println!("nested {}", nested(3));
    println!("fallthrough {}", nested(0));
    let x = 'same: {
        let y = 'same: { break 'same 7; };
        break 'same y + 1;
    };
    println!("shadow {}", x);
    for early in [true, false] {
        let value = moved(early);
        println!("moved {}", value.0);
        let a = [1, 2];
        let b = [3, 4, 5];
        let r = slice(early, &a, &b);
        println!("slice {} {}", r.len(), r[0]);
        println!("init {}", initialized(early));
    }
    let mut n = 1;
    let r = 'reference: { break 'reference &mut n; };
    *r += 4;
    println!("reference {}", n);
    let mut total = 0;
    'outer: for i in 0..4 {
        'block: {
            if i == 1 { continue 'outer; }
            if i == 3 { break 'outer; }
            total += i;
            break 'block;
        }
    }
    println!("for {}", total);
}
