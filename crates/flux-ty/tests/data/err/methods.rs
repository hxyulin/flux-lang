struct S {
    a: i32,
}

impl S {
    fn new() -> S {
        S { a: 0 }
    }

    fn make(a: i32) -> S {
        S { a }
    }

    fn get(&self) -> i32 {
        self.a
    }

    fn set(&mut self, a: i32) {
        self.a = a;
    }

    fn missing_field(&self) -> i32 {
        self.b
    }
}

fn no_method(s: S) {
    s.foo();
}

fn no_method_ref(s: S) {
    let r = &s;
    r.foo();
}

fn assoc_as_method(s: S) {
    s.new();
}

fn field_as_method(s: S) -> i32 {
    s.a()
}

fn method_value(s: S) -> i32 {
    s.get
}

fn too_few(mut s: S) {
    s.set();
}

fn too_many(s: S) -> i32 {
    s.get(1)
}

fn assoc_too_few() -> S {
    S::make()
}

fn ufcs_too_many(s: S) -> i32 {
    S::get(&s, 1)
}

fn ufcs_no_autoref(s: S) -> i32 {
    S::get(s)
}

fn wrong_arg(mut s: S) {
    s.set(true);
}

fn takes_mut(s: &mut S) {
    s.a = 1;
}

fn shared_for_mut(s: S) {
    takes_mut(&s);
}

fn int_for_ref(r: &mut i32) {
    *r = 1;
    let _x: &mut i32 = 5;
}

fn deref_int() -> i32 {
    let x = 1;
    *x
}

fn field_through_ref(s: &S) -> i32 {
    s.b
}

fn ref_in_mut_binding(x: i32) -> i32 {
    let mut r = &x;
    *r
}

fn ref_declared_later(x: i32) -> i32 {
    let r;
    r = &x;
    *r
}

fn ref_in_tuple(x: i32) -> i32 {
    let t = (&x, 1);
    *t.0
}

fn ref_to_ref(x: i32) -> i32 {
    let r = &&x;
    **r
}

fn returns_ref(r: &i32) -> &i32 {
    r
}

fn break_ref(x: i32) -> i32 {
    let r = loop {
        break &x;
    };
    *r
}

fn reassign_ref_local() -> i32 {
    let x = 1;
    let y = 2;
    let r = &x;
    r = &y;
    *r
}

fn reassign_ref_param(r: &mut i32, s: &mut i32) -> i32 {
    r = s;
    *r
}

struct Holder {
    r: &i32,
}

fn no_lifetime() -> &i32 {
    &1
}
