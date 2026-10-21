pub struct S {
    a: i32,
    b: i32,
}

pub struct Big {
    s: S,
    n: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.a
    }

    fn set(&mut self, v: i32) {
        self.a = v;
    }

    fn add(&mut self, v: i32) -> i32 {
        self.a += v;
        self.a
    }

    fn both(&mut self) -> i32 {
        let x = &mut self.a;
        let y = &self.b;
        *x += *y;
        self.set(1);
        self.get()
    }
}

fn add_to(a: &mut i32, b: &i32) {
    *a += *b;
}

fn swap_add(a: &mut i32, b: &mut i32) {
    *a += *b;
    *b = 0;
}

fn inc(r: &mut S) {
    r.a += 1;
}

pub fn two_phase(mut s: S) -> i32 {
    s.set(s.get());
    s.set(s.a + 1);
    s.add(1)
}

pub fn two_phase_through_ref(r: &mut S) {
    r.set(r.get() + 1);
    r.set(r.a);
}

pub fn disjoint_fields(mut a: S) -> i32 {
    add_to(&mut a.a, &a.b);
    let x = &mut a.a;
    let y = &mut a.b;
    swap_add(x, y);
    a.a
}

pub fn disjoint_through_ref(r: &mut S) {
    swap_add(&mut r.a, &mut r.b);
}

pub fn nested_fields(mut big: Big) -> i32 {
    let s = &mut big.s;
    big.n = 3;
    s.a = big.n;
    let n = &big.n;
    big.s.b = *n;
    big.s.a + big.s.b
}

pub fn mutate_after_last_use(mut x: i32) -> i32 {
    let r = &x;
    let y = *r;
    x = y + 1;
    x
}

pub fn mutate_after_unused_borrow(mut x: i32) -> i32 {
    let _r = &x;
    x += 1;
    x
}

pub fn mut_borrow_then_original(mut s: S) -> i32 {
    let r = &mut s;
    r.a = 2;
    s.a += 1;
    s.a
}

pub fn reborrow_then_original(r: &mut S) -> i32 {
    let r2 = &mut *r;
    r2.a = 1;
    r.a = 2;
    let r3 = &*r;
    let v = r3.a;
    r.b = v;
    r.a
}

pub fn implicit_reborrow(r: &mut S) {
    inc(r);
    inc(r);
    r.a += 1;
}

pub fn annotated_reborrow(r: &mut S) -> i32 {
    let r2: &mut S = r;
    r2.a = 1;
    r.a = 2;
    r.a
}

pub fn let_underscore_deref(r: &Big) -> i32 {
    let _ = *r;
    let _ = r.s;
    r.n
}

pub fn copy_out(r: &S) -> i32 {
    let x = r.a;
    let y = *&r.b;
    x + y
}

pub fn borrows_in_loop(mut x: i32) -> i32 {
    let mut i = 0;
    while i < 3 {
        let r = &mut x;
        *r += 1;
        let s = &x;
        i += *s;
    }
    loop {
        let r = &x;
        let y = *r;
        x += y + 1;
        if x > 10 {
            break;
        }
    }
    x
}

pub fn loop_then_break(mut x: i32) -> i32 {
    let r = &x;
    loop {
        let y = *r;
        x += y + 1;
        break;
    }
    x
}

pub fn loop_reads_ref(mut x: i32) -> i32 {
    let r = &x;
    let mut n = 0;
    loop {
        n += *r + 1;
        if n > 5 {
            break;
        }
    }
    x += n;
    x
}

pub fn use_in_branch(mut x: i32, c: bool) -> i32 {
    let r = &x;
    if c {
        x = *r + 1;
    }
    x += 1;
    x
}

pub fn dead_in_then(mut x: i32, c: bool) -> i32 {
    let r = &x;
    if c {
        x = 5;
    } else {
        x = *r + 1;
    }
    x
}

pub fn dead_after_else(mut x: i32, c: bool) -> i32 {
    let r = &x;
    if c {
        x = 5
    } else {
        let y = *r;
        x = y + 1
    }
    x += 1;
    x
}

pub fn borrowed_if(mut x: i32, y: i32, c: bool) -> i32 {
    let r = if c { &x } else { &y };
    let z = *r;
    x = 2;
    x + z
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

pub fn diverging_branch(mut s: S, c: bool) -> i32 {
    let r = &s;
    if c {
        let m = &mut s;
        m.a = 1;
        unsafe { exit(m.a) }
    }
    r.a
}

pub fn temporaries(x: i32) -> i32 {
    let r = &S { a: x, b: 1 };
    let t = &S { a: 1, b: 2 }.a;
    let u = &mut 5;
    *u += 1;
    r.get() + *t + *u
}

pub fn method_result_then_mutate(mut s: S) -> i32 {
    let v = s.get();
    s.a = v + 1;
    let w = (&s).get();
    s.add(w)
}

pub fn self_methods(mut s: S) -> i32 {
    s.both()
}

pub struct T {
    v: i32,
}

pub struct W {
    a: i32,
    inner: T,
}

impl T {
    fn bump(&mut self) {
        self.v += 1;
    }
}

fn make_w() -> W {
    W {
        a: 1,
        inner: T { v: 2 },
    }
}

pub fn field_borrow_move_other(w: W) -> i32 {
    let r = &w.a;
    let t = w.inner;
    *r + t.v
}

pub fn move_then_reinit(mut w: W) -> i32 {
    let t = w;
    w = make_w();
    let r = &w;
    r.a + t.a
}

pub fn read_then_assign_from_mut_ref(mut x: i32) -> i32 {
    let r = &mut x;
    x = *r;
    x
}

pub fn disjoint_tuple_fields() -> i32 {
    let mut t = (1, 2);
    let r = &mut t.0;
    t.1 = 3;
    *r = 1;
    t.0 + t.1
}

pub fn borrow_in_condition(mut x: i32) -> i32 {
    if {
        let r = &mut x;
        *r += 1;
        *r > 0
    } {
        x = 1
    }
    x
}

pub fn break_value_from_ref(mut x: i32) -> i32 {
    let v = loop {
        let r = &x;
        break *r;
    };
    x = 1;
    x + v
}

pub fn diverging_mutation(mut x: i32, c: bool) -> i32 {
    let r = &mut x;
    if c {
        x = 1;
        unsafe { exit(x) }
    }
    *r = 2;
    x
}

pub fn method_on_other_field(mut w: W) -> i32 {
    let r = &w.a;
    w.inner.bump();
    let m = &mut w.inner;
    let a = w.a;
    m.bump();
    *r + a
}

pub fn conditional_mut_refs(mut x: i32, mut y: i32, c: bool) -> i32 {
    let r = if c { &mut x } else { &mut y };
    *r += 1;
    x + y
}

pub fn borrow_per_iteration(mut x: i32) -> i32 {
    let mut i = 0;
    while i < 3 {
        let r = &x;
        if i == 1 {
            x = 0;
        } else {
            i += *r;
        }
        i += 1;
    }
    i
}

pub fn mut_ref_then_shared(mut s: S) -> i32 {
    let r = &mut s;
    r.a = 1;
    let r2 = &s;
    r2.a
}

pub fn other_field_through_ref(r: &mut S) -> i32 {
    let b = &r.b;
    r.a = 1;
    *b
}

pub fn fields_through_ref(w: &mut W) -> i32 {
    let r = &mut w.inner;
    r.bump();
    let a = &w.a;
    let v = &mut w.inner.v;
    *v += *a;
    *v
}

pub fn temporary_receivers() -> i32 {
    make_w().inner.bump();
    let r = &mut make_w();
    r.a = 1;
    let q = &mut make_w().a;
    *q += 1;
    let k = &make_w().inner;
    r.a + *q + k.v
}

pub fn nested_derefs(mut x: i32) -> i32 {
    let y = *&mut *&mut x;
    *&mut x = 3;
    *&*&x + y
}

pub fn copy_shared_refs(s: S) -> i32 {
    let r = &s;
    let r2 = r;
    let r3 = &*r2;
    r.a + r2.a + r3.get()
}

pub fn block_value_borrow(mut s: S) -> i32 {
    let v = {
        let r = &mut s;
        r.a = 3;
        r.a
    };
    s.set(v);
    s.get()
}
