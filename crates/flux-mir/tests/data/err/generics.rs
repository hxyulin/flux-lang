// Moves and borrows of generic values: a `T` is never `Copy`.

struct Pair<T> {
    a: T,
    b: T,
}

struct Wrap<T>(T);

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }

    fn into_a(self) -> T {
        self.a
    }

    fn set_a(&mut self, a: T) {
        self.a = a;
    }

    fn take_b(&self) -> T {
        self.b
    }

    fn set_through_shared(&self, a: T) {
        self.a = a;
    }

    fn set_from_shared(&self, a: T) {
        self.set_a(a);
    }
}

impl Pair<i32> {
    fn total(&self) -> i32 {
        self.a + self.b
    }

    fn bump(&mut self, by: i32) {
        self.a += by;
    }
}

fn id<T>(x: T) -> T {
    x
}

fn take<T>(_x: T) {}

// Use of moved values.
fn twice<T>(x: T) -> Pair<T> {
    Pair::new(x, x)
}

fn after_call<T>(x: T) -> T {
    take(x);
    x
}

fn after_let<T>(x: T) -> T {
    let y = x;
    let _z = y;
    x
}

fn in_loop<T>(x: T) {
    loop {
        take(x);
    }
}

fn in_while<T>(x: T, n: i32) {
    let mut i = 0;
    while i < n {
        let _y = id(x);
        i += 1;
    }
}

fn one_branch<T>(x: T, flag: bool) -> T {
    if flag {
        take(x);
    }
    x
}

fn struct_after_move<T>(p: Pair<T>) -> Pair<T> {
    let q = p;
    let _ = q;
    p
}

// Partial moves of generic struct fields.
fn field_twice<T>(p: Pair<T>) {
    take(p.a);
    take(p.a);
}

fn whole_after_field<T>(p: Pair<T>) -> Pair<T> {
    take(p.a);
    p
}

fn method_after_field<T>(p: Pair<T>) -> T {
    let _a = p.a;
    p.into_a()
}

fn field_in_loop<T>(p: Pair<T>) {
    loop {
        take(p.b);
    }
}

fn nested_field<T>(p: Pair<Pair<T>>) -> Pair<Pair<T>> {
    let _inner = p.a.b;
    p
}

fn tuple_struct_field<T>(w: Wrap<T>) -> Wrap<T> {
    take(w.0);
    w
}

fn pattern_then_whole<T>(p: Pair<T>) -> Pair<T> {
    let Pair { a, .. } = p;
    take(a);
    p
}

// An instance is not `Copy` either, even when its fields are.
fn instance_twice(p: Pair<i32>) -> i32 {
    let x = p.a + p.a;
    take(p);
    take(p);
    x
}

fn instance_field_after_move(p: Pair<Pair<u8>>) -> u8 {
    let a = p.a;
    let b = p.a.a;
    a.a + b
}

// Moving out of references.
fn out_of_ref<T>(r: &T) -> T {
    *r
}

fn field_out_of_ref<T>(p: &Pair<T>) -> T {
    p.a
}

fn field_out_of_mut_ref<T>(p: &mut Pair<T>) -> T {
    p.b
}

fn method_out_of_ref<T>(p: &Pair<T>) -> T {
    p.into_a()
}

// Conflicting borrows.
fn move_while_borrowed<T>(p: Pair<T>) {
    let r = &p.a;
    take(p);
    let _r2 = r;
}

fn assign_while_borrowed<T>(mut p: Pair<T>, x: T) {
    let r = &p.a;
    p.a = x;
    let _r2 = r;
}

fn two_mut<T>(mut p: Pair<T>) {
    let r = &mut p;
    let s = &mut p;
    let _r2 = r;
    let _s2 = s;
}

fn mut_method_while_borrowed<T>(mut p: Pair<T>, x: T) {
    let r = &p.a;
    p.set_a(x);
    let _r2 = r;
}

fn not_mut<T>(p: Pair<T>, x: T) {
    p.set_a(x);
}

fn not_mut_field<T>(p: Pair<T>, x: T) {
    p.a = x;
}

// Two-phase borrows do not cover a mutable borrow in the arguments.
fn two_phase_mut_arg(mut p: Pair<i32>) {
    p.bump({
        let q = &mut p;
        q.a
    });
}

fn main() {}
