struct S {
    a: i32,
    b: i32,
}

struct T {
    v: i32,
}

struct W {
    inner: T,
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

    fn take(self) -> i32 {
        self.a
    }

    fn both(&mut self, r: &i32) {
        self.a += *r;
    }

    fn assign_through_shared_self(&self) {
        self.a = 1;
    }

    fn mut_method_from_shared_self(&self) -> i32 {
        self.set(1);
        self.a
    }

    fn by_value_from_shared_self(&self) -> i32 {
        self.take()
    }

    fn assign_by_value_self(self) -> i32 {
        self.a = 1;
        self.a
    }

    fn two_mut_self(&mut self) {
        pair(&mut *self, &mut *self);
    }

    fn use_self_after_move(self) -> i32 {
        let n = consume(self);
        n + self.a
    }
}

impl W {
    fn move_inner(&self) -> T {
        self.inner
    }
}

fn consume(s: S) -> i32 {
    s.a
}

fn pair(a: &mut S, b: &mut S) {
    a.a += b.a;
}

fn add_to(a: &mut i32, b: &i32) {
    *a += *b;
}

fn add_whole(a: &mut S, b: &i32) {
    a.a += *b;
}

fn swap_add(a: &mut i32, b: &mut i32) {
    *a += *b;
}

fn set_free(r: &mut S, v: i32) {
    r.a = v;
}

fn take_t(t: T) -> i32 {
    t.v
}

fn inc(x: &mut i32) {
    *x += 1;
}

// E0499

fn two_mut() {
    let mut x = 0;
    let a = &mut x;
    let b = &mut x;
    *b = 1;
    *a = 2;
}

fn nested_mut_calls(mut s: S) -> i32 {
    s.add(s.add(1))
}

fn mut_args_twice() {
    let mut a = 1;
    swap_add(&mut a, &mut a);
}

fn same_ref_twice(r: &mut S) {
    pair(r, r);
}

fn mut_borrow_held_in_loop() {
    let mut x = 0;
    let r = &mut x;
    let mut i = 0;
    while i < 2 {
        let m = &mut x;
        *m += 1;
        *r += 1;
        i += 1;
    }
}

// E0502

fn mut_while_shared() {
    let mut x = 0;
    let a = &x;
    let b = &mut x;
    *b = 1;
    let _ = *a;
}

fn shared_while_mut() {
    let mut x = 0;
    let b = &mut x;
    let a = &x;
    *b = 1;
    let _ = *a;
}

fn mut_method_while_shared(mut s: S) -> i32 {
    let r = &s;
    s.set(1);
    r.a
}

fn shared_method_while_mut(mut s: S) {
    let r = &mut s;
    s.get();
    r.a = 1;
}

fn ufcs_explicit_mut(mut s: S) {
    S::set(&mut s, s.get());
}

fn shared_arg_held_at_call(mut s: S) {
    s.both(&s.a);
}

fn fn_args_mut_and_shared() {
    let mut a = 1;
    add_to(&mut a, &a);
}

fn fn_args_whole_and_field(mut a: S) {
    add_whole(&mut a, &a.b);
}

fn field_then_whole_method(mut s: S) {
    let ra = &mut s.a;
    let y = s.get();
    *ra += y;
}

fn two_phase_receiver_while_mut_borrowed(mut s: S) {
    let r = &mut s;
    s.set(r.a);
}

fn write_in_two_phase_args(mut s: S) {
    s.set({
        s.a = 5;
        1
    });
}

fn mut_reborrow_then_shared_whole(mut s: S) -> i32 {
    let r = &mut s.a;
    let r2 = &s;
    *r += r2.b;
    *r
}

fn mut_field_then_self_method(s: &mut S) -> i32 {
    let a = &mut s.a;
    let v = s.get();
    *a = v;
    v
}

// E0503

fn compound_assign_while_mut_borrowed(mut x: i32) {
    let r = &mut x;
    x += 1;
    *r = 2;
}

fn compound_assign_field_while_mut_borrowed(mut s: S) {
    let r = &mut s.a;
    s.a += 1;
    *r = 2;
}

fn compound_assign_while_conditionally_borrowed(mut x: i32, mut y: i32, c: bool) -> i32 {
    let r = if c { &mut x } else { &mut y };
    y += 1;
    *r += 1;
    x + y
}

fn read_into_mut_ref(mut x: i32) -> i32 {
    let r = &mut x;
    *r = x;
    *r
}

fn use_while_mut_borrowed() {
    let mut x = 0;
    let r = &mut x;
    let y = x + 1;
    *r = y;
}

fn field_read_while_mut_borrowed(mut s: S) {
    let r = &mut s;
    let y = s.a;
    r.a = y;
}

fn explicit_mut_arg_then_read(mut s: S) {
    set_free(&mut s, s.a);
}

fn reborrow_read_original(r: &mut S) -> i32 {
    let r2 = &mut *r;
    let x = r.a;
    r2.a = 2;
    x
}

fn loop_read_while_mut_borrowed() -> i32 {
    let mut x = 0;
    let r = &mut x;
    let mut n = 0;
    while n < 3 {
        *r += 1;
        n += x;
    }
    n
}

// E0506

fn assign_borrowed() -> i32 {
    let mut x = 0;
    let r = &x;
    x = 1;
    *r + x
}

fn compound_assign_borrowed() -> i32 {
    let mut x = 0;
    let r = &x;
    x += 1;
    *r
}

fn field_assign_whole_borrowed(mut s: S) -> i32 {
    let r = &s;
    s.a = 1;
    r.a
}

fn field_assign_while_mut_borrowed(mut s: S) -> i32 {
    let r = &mut s;
    s.a = 1;
    r.a = 2;
    s.a
}

fn assign_through_original_while_reborrowed(r: &mut S) {
    let r2 = &mut *r;
    r.a = 1;
    r2.a = 2;
}

fn assign_while_shared_reborrow(r: &mut S) -> i32 {
    let r2 = &*r;
    r.a = 1;
    r2.a
}

fn copied_ref_keeps_loan() -> i32 {
    let mut x = 0;
    let r = &x;
    let r2 = r;
    x = 1;
    *r2 + x
}

fn original_ref_keeps_loan() -> i32 {
    let mut x = 0;
    let r = &x;
    let r2 = r;
    let y = *r2;
    x = 1;
    *r + x + y
}

fn assign_in_branch_use_after(c: bool) -> i32 {
    let mut x = 0;
    let r = &x;
    if c {
        x = 5;
    }
    *r + x
}

fn ref_from_if(c: bool) -> i32 {
    let mut x = 0;
    let mut y = 1;
    let r = if c { &x } else { &y };
    x = 2;
    y += *r;
    x + y
}

fn loop_use_ref_and_mutate() -> i32 {
    let mut x = 0;
    let r = &x;
    loop {
        let y = *r;
        x += 1;
        if x > y + 3 {
            break;
        }
    }
    x
}

fn while_use_ref_and_mutate() -> i32 {
    let mut x = 0;
    let r = &x;
    while x < 3 {
        x += *r + 1;
    }
    x
}

fn assign_whole_while_field_borrowed(mut s: S) -> i32 {
    let r = &s.a;
    s = S { a: 1, b: 2 };
    *r
}

fn reborrow_chain(mut x: i32) -> i32 {
    let a = &mut x;
    let b = &mut *a;
    let c = &*b;
    *a = 1;
    *c
}

fn while_condition_uses_ref(mut x: i32) {
    let r = &x;
    while *r < 3 {
        x += 1;
    }
}

fn assign_in_nested_branch_of_loop(mut x: i32) -> i32 {
    let r = &x;
    let mut i = 0;
    while i < 3 {
        if i == 1 {
            x = 0;
        }
        i += *r;
    }
    i
}

fn loop_mutate_ref_used_after() -> i32 {
    let mut x = 0;
    let r = &x;
    loop {
        x += 1;
        if x > 3 {
            break;
        }
    }
    *r
}

// E0505

fn move_while_borrowed(s: S) -> i32 {
    let r = &s;
    let t = s;
    r.a + t.a
}

fn move_into_call_while_field_borrowed(s: S) -> i32 {
    let r = &s.a;
    consume(s);
    *r
}

fn by_value_method_while_borrowed(s: S) -> i32 {
    let r = &s;
    s.take();
    r.a
}

fn move_mut_ref_while_reborrowed(r: &mut S) {
    let r2 = &mut *r;
    let r3 = r;
    r2.a = 1;
    pair(r3, r3);
}

fn move_inside_block_while_borrowed(s: S) -> i32 {
    let r = &s;
    let n = {
        let t = s;
        t.a
    };
    n + r.a
}

// E0594

fn assign_through_shared(r: &i32) {
    *r = 1;
}

fn assign_field_through_shared(r: &S) {
    r.a = 1;
}

fn assign_deref_field_through_shared(r: &S) {
    (*r).a = 1;
}

fn assign_through_local_shared_ref() -> i32 {
    let mut x = 0;
    let r = &x;
    *r = 1;
    x = 2;
    x
}

fn assign_field_of_non_mut_local() -> i32 {
    let s = S { a: 0, b: 0 };
    s.a = 1;
    s.a
}

// E0596

fn mut_borrow_of_non_mut_local() {
    let x = 0;
    let r = &mut x;
    *r = 1;
}

fn mut_reborrow_of_shared(r: &i32) {
    let m = &mut *r;
    *m = 1;
}

fn mut_borrow_field_through_shared(r: &S) {
    inc(&mut r.a);
}

fn mut_method_on_non_mut_local() -> i32 {
    let s = S { a: 1, b: 2 };
    s.set(2);
    s.a
}

fn mut_method_through_shared(r: &S) {
    r.set(2);
}

// E0507

fn move_out_of_deref(r: &T) -> i32 {
    let t = *r;
    t.v
}

fn move_out_of_mut_deref(r: &mut T) -> i32 {
    let t = *r;
    t.v
}

fn move_out_of_field(r: &W) -> i32 {
    let t = r.inner;
    t.v
}

fn pass_deref_by_value(r: &T) -> i32 {
    take_t(*r)
}

fn by_value_method_through_ref(r: &S) -> i32 {
    r.take()
}

fn by_value_method_through_mut(r: &mut S) -> i32 {
    r.take()
}

fn deref_statement(r: &T) {
    *r;
}

// E0382, E0381, E0597

fn borrow_of_moved(s: S) -> i32 {
    let t = s;
    let r = &s;
    r.a + t.a
}

fn method_on_moved(s: S) -> i32 {
    s.take();
    s.get()
}

fn by_value_method_twice(s: S) -> i32 {
    s.take();
    s.take()
}

fn move_mut_ref_by_let(r: &mut S) {
    let r2 = r;
    r2.a = 1;
    r.a = 2;
}

fn borrow_partially_moved(w: W) -> i32 {
    let t = w.inner;
    let r = &w;
    r.inner.v + t.v
}

fn borrow_maybe_uninit(c: bool) -> i32 {
    let x: i32;
    if c {
        x = 1;
    }
    let r = &x;
    *r
}

fn borrow_uninit() -> i32 {
    let x: i32;
    let r = &x;
    *r
}

fn mut_borrow_uninit() {
    let mut x: i32;
    let r = &mut x;
    *r = 1;
}

fn block_tail_borrow() -> i32 {
    let r = {
        let x = 1;
        &x
    };
    *r
}

fn block_tail_field_borrow() -> i32 {
    let r = {
        let s = S { a: 1, b: 2 };
        &s.a
    };
    *r
}

fn block_tail_borrow_in_branch(c: bool) -> i32 {
    let x = 1;
    let r = if c {
        let y = 2;
        &y
    } else {
        &x
    };
    *r
}

fn block_tail_borrow_in_loop() -> i32 {
    let mut i = 0;
    let mut sum = 0;
    while i < 2 {
        let r = {
            let y = i;
            &y
        };
        sum += *r;
        i += 1;
    }
    sum
}
