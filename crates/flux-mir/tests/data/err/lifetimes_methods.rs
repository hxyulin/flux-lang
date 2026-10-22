struct Pair {
    a: i32,
    b: i32,
}

impl Pair {
    fn first(&self) -> &i32 {
        &self.a
    }
}

struct W<'a> {
    r: &'a mut i32,
}

trait Get {
    fn get(&self) -> &i32;
}

impl Get for Pair {
    fn get(&self) -> &i32 {
        &self.b
    }
}

fn mutate_while_method_borrowed() {
    let mut p = Pair { a: 1, b: 2 };
    let r = p.first();
    p.a = 5;
    let _ = *r;
}

fn mutate_while_struct_holds() {
    let mut x = 1;
    let w = W { r: &mut x };
    x = 2;
    *w.r = 3;
}

fn through_shared<'a, 'b>(w: &'b W<'a>) -> &'a i32 {
    &*w.r
}

fn to_static<'a>(x: &'a i32) -> &'static i32 {
    x
}

fn invariant<'a, 'b>(x: &'a mut &'b i32, y: &'a i32) {
    *x = y;
}

fn first<T>(v: &[T]) -> &T {
    &v[0]
}

fn generic_then_move() {
    let mut v = [1, 2];
    let f = first(&v);
    v[0] = 3;
    let _ = *f;
}

fn trait_then_mutate() {
    let mut p = Pair { a: 1, b: 2 };
    let g = Get::get(&p);
    p.b = 3;
    let _ = *g;
}

fn generic_trait<T: Get>(t: T) -> i32 {
    let r = t.get();
    drop(t);
    *r
}

fn closure_returns_capture() {
    let mut x = 1;
    let c = || &x;
    let r = c();
    x = 2;
    let _ = *r;
}

fn option_ref() {
    let mut v = [1, 2, 3];
    let o: Option<&i32> = Some(&v[0]);
    v[1] = 4;
    let _ = o;
}

fn break_ref() {
    let r;
    {
        let x = 1;
        r = loop {
            break &x;
        };
    }
    let _ = *r;
}

fn main() {}
