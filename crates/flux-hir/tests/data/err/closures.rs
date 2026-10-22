// Closures and loops: `break` and `continue` cannot leave a closure,
// labels must name an enclosing loop, and inclusive ranges need an end.

fn break_in_closure() {
    loop {
        let f = || break;
        let g = || continue;
    }
    let h = || break;
}

fn labels() {
    'a: for i in 0..3 {
        break 'b;
    }
    loop {
        continue 'a;
    }
}

fn params(f: i32) {
    let c = |x: i32, x: i32| x;
    let d = async || 1;
}

fn main() {}
