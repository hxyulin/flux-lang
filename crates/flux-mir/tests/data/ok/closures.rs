// Closures as MIR: what each captures, by reference or by value, its
// body as its kind's trait method, and the shims for the others; calls
// of closures and of `Fn`-bounded parameters; and a `for` loop.

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {}
}

struct Point {
    x: i32,
    y: i32,
}

fn call<F: Fn(i32) -> i32>(f: F) -> i32 {
    f(1)
}

fn call_mut<F: FnMut()>(mut f: F) {
    f();
}

fn call_once<F: FnOnce() -> i32>(f: F) -> i32 {
    f()
}

fn captures(k: i32) -> i32 {
    let by_ref = |x| x + k;
    let mut p = Point { x: 1, y: 2 };
    call_mut(|| p.x += 1);
    let n = Noisy(3);
    let by_value = move || {
        let m = n;
        m.0
    };
    call(by_ref) + call_once(by_value) + p.y
}

fn loops() -> i32 {
    let mut total = 0;
    for i in 0..3 {
        total += i;
    }
    total
}

fn main() {}
