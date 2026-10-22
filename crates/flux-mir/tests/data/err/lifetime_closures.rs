fn closure_stores_capture() {
    let a = 1;
    let mut r: &i32 = &a;
    {
        let short = 5;
        let s = &short;
        let mut c = || {
            r = s;
        };
        c();
    }
    let _ = *r;
}

fn closure_stores_arg() {
    let a = 1;
    let mut r: &i32 = &a;
    {
        let short = 5;
        let mut c = |x: &i32| {
            let _ = x;
        };
        c(&short);
    }
    let _ = *r;
    r = &a;
    let _ = r;
}

fn closure_returns_arg() {
    let r;
    {
        let short = 5;
        let c = |x: &i32| -> i32 { *x };
        r = c(&short);
    }
    let _ = r;
}

fn returns_closure_ref<'a>(v: &'a [i32]) -> &'a i32 {
    let c = |i: usize| &v[i];
    c(0)
}

struct Wrap<'a> {
    inner: &'a mut Vec2,
}

struct Vec2 {
    x: i32,
    y: i32,
}

impl<'a> Wrap<'a> {
    fn x(&mut self) -> &mut i32 {
        &mut self.inner.x
    }
    fn both(&mut self) -> (&mut i32, &mut i32) {
        (&mut self.inner.x, &mut self.inner.y)
    }
}

fn wrap_twice() {
    let mut v = Vec2 { x: 1, y: 2 };
    let mut w = Wrap { inner: &mut v };
    let a = w.x();
    let b = w.x();
    *a += 1;
    *b += 1;
}

fn wrap_ok() {
    let mut v = Vec2 { x: 1, y: 2 };
    let mut w = Wrap { inner: &mut v };
    let (a, b) = w.both();
    *a += *b;
    let _ = v.x;
}

fn main() {}
