//! Closures: signatures from annotations, bounds and calls; captures by
//! reference (whole bindings and single fields) and by value, with
//! `move`; `Fn`, `FnMut` and `FnOnce` closures called directly, through
//! generic parameters and references, from struct fields, and through
//! the shims of the traits beyond their own kind; nested closures; and
//! drops of what closures own.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(100, self.0);
    }
}

struct Point {
    x: i32,
    y: i32,
}

struct Holder<F> {
    f: F,
}

impl<F: Fn(i32) -> i32> Holder<F> {
    fn run(&self, x: i32) -> i32 {
        (self.f)(x)
    }
}

fn twice<F: FnMut()>(mut f: F) {
    f();
    f();
}

fn once<F: FnOnce() -> i32>(f: F) -> i32 {
    f()
}

fn compose<A: Fn(i32) -> i32, B: Fn(i32) -> i32>(a: A, b: B, x: i32) -> i32 {
    b(a(x))
}

fn by_ref<F: Fn(i32) -> i32>(f: &F, x: i32) -> i32 {
    f(x)
}

fn generic_owner<T: Copy>(t: T, n: i32) -> i32 {
    let count = |_: T| n + 1;
    count(t) + count(t)
}

fn main() {
    // Inferred from a later call.
    let add = |a, b| a + b;
    show(1, add(2, 3));

    // Nested closures.
    let base = 10;
    let outer = |x: i32| {
        let inner = |y: i32| y + base;
        inner(x) * 2
    };
    show(2, outer(1));

    // Mutating captured state through FnMut.
    let mut counter = 0;
    twice(|| counter += 5);
    show(3, counter);

    // Disjoint field captures.
    let mut p = Point { x: 1, y: 2 };
    let mut bump = || p.x += 10;
    let ry = &mut p.y;
    *ry += 1;
    bump();
    show(4, p.x * 100 + p.y);

    // Moving a value with a destructor into an FnOnce.
    let n = Noisy(7);
    let r = once(move || {
        let m = n;
        m.0 * 2
    });
    show(5, r);

    // A move closure that does not consume is called through call_once.
    let k = Noisy(8);
    let r = once(move || k.0 + 1);
    show(6, r);

    // Closures in structs.
    let factor = 3;
    let h = Holder { f: |x| x * factor };
    show(7, h.run(5));

    show(8, compose(|x| x + 1, |x| x * 10, 4));
    let sq = |x: i32| x * x;
    show(9, by_ref(&sq, 9));
    show(10, generic_owner(true, 4));

    // Copy closures may be used twice.
    let c = |x: i32| x - 1;
    show(11, compose(c, c, 10));

    // Patterns in parameters.
    let swap = |(a, b): (i32, i32)| (b, a);
    let (s, t) = swap((1, 2));
    show(12, s * 10 + t);

    // Returning early from a closure.
    let clamp = |x: i32| -> i32 {
        if x > 5 {
            return 5;
        }
        x
    };
    show(13, clamp(9) + clamp(2));

    // Drops of captured values at the end of the closure's scope.
    let d = Noisy(9);
    let peek = || d.0;
    show(14, peek());
    let owned = Noisy(11);
    let keep = move || owned.0;
    show(15, keep());
}
