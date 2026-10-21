// Where drops go: bindings at the end of their block in reverse order,
// then parameters; temporaries at the end of their statement, or of the
// block when a `let` borrows them; a value moved out is not dropped, one
// moved out on some paths has a drop flag, and one with fields moved out
// drops the others; assignment drops the old value first; a type
// parameter may need dropping unless it is `Copy`.

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {}
}

impl Noisy {
    fn get(&self) -> i32 {
        self.0
    }
}

struct Pair(Noisy, Noisy);

fn make() -> Noisy {
    Noisy(0)
}

fn scopes(a: Noisy) {
    let b = Noisy(1);
    let c = Noisy(2);
    let _ = make();
    make().get();
}

fn extended() -> i32 {
    let r = &make();
    r.0
}

fn moved(a: Noisy, cond: bool) {
    let b = a;
    if cond {
        drop(b);
    }
}

fn partial(p: Pair) -> Noisy {
    p.1
}

fn assign(mut a: Noisy, r: &mut Noisy) {
    a = Noisy(1);
    *r = Noisy(2);
}

fn generic<T, C: Copy>(t: T, c: C) {}

fn main() {
    scopes(Noisy(3));
    extended();
    moved(Noisy(4), true);
    partial(Pair(Noisy(5), Noisy(6)));
    assign(Noisy(7), &mut Noisy(8));
    generic(Noisy(9), 1);
}
