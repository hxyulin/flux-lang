//! What a closure captures, and how, as rustc's capture analysis says,
//! seen in the order values are dropped. A by-value use of a `Copy` part
//! of an element (`p.1[1].0.is_positive()`, `let x = p[0].0`, a pattern
//! binding it) borrows the indexed array; with `move`, places are moved
//! up to the first dereference, a value of a type with a `Drop` impl
//! whole unless the part is `Copy`; captures are ordered by binding, in
//! the order each is first used, then by field; indices are evaluated
//! after the place they index; a raw pointer is borrowed shared even to
//! write through it (an `Fn` closure, the pointer not `mut`); a place
//! behind a `&mut` behind a `&` is captured itself. Also overloaded
//! indexing, nested closures and `Copy` method receivers.

use core::ops::{Index, IndexMut};

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

#[derive(Clone, Copy)]
struct C(i32);

struct Pair {
    a: D,
    b: D,
}

struct Guarded {
    n: i32,
    inner: D,
}

impl Drop for Guarded {
    fn drop(&mut self) {
        println!("drop guarded {}", self.n);
    }
}

fn call<T>(f: impl FnOnce() -> T) -> T {
    f()
}

// A by-value method call on a `Copy` part of an indexed place borrows
// the array: nothing moves into the closure.
fn copy_method_receiver(p: (D, [D; 2])) -> Option<i32> {
    Some(0).map(|_x: i32| if p.1[1].0.is_positive() { 7 } else { 8 })
}

fn copy_field_read(p: (D, [D; 2])) -> Option<i32> {
    Some(0).map(|_x: i32| if p.1[1].0 > 0 { 7 } else { 8 })
}

fn copy_let(p: (D, [D; 2])) -> i32 {
    call(|| {
        let x = p.1[0].0;
        x + 1
    })
}

fn copy_tuple_pattern(p: (D, [(i32, i32); 2])) -> i32 {
    call(|| {
        let (a, b) = p.1[1];
        a + b
    })
}

fn copy_match(p: (D, [C; 2])) -> i32 {
    call(|| match p.1[0] {
        C(n) if n > 0 => n,
        C(n) => -n,
    })
}

fn copy_struct_receiver(p: (D, [C; 2])) -> i32 {
    call(|| p.1[1].get())
}

impl C {
    fn get(self) -> i32 {
        self.0
    }
}

fn ref_method(p: (D, [D; 2])) -> i32 {
    call(|| p.1[0].value())
}

impl D {
    fn value(&self) -> i32 {
        self.0
    }
    fn index(&self) -> usize {
        self.0 as usize % 2
    }
    fn bump(&mut self) {
        self.0 += 100;
    }
}

fn mut_method(mut p: (D, [D; 2])) -> i32 {
    let mut f = || p.1[1].bump();
    f();
    f();
    p.1[1].0
}

fn index_by_capture(p: (D, [D; 3]), i: (usize, D)) -> i32 {
    call(|| p.1[i.0].0)
}

fn nested_index(p: [[C; 2]; 2], i: usize, j: usize) -> i32 {
    call(|| p[i][j].0)
}

// With `move`, the place is moved: up to the first dereference, and
// whole when it is a field of a type with a `Drop` impl.
fn move_field(p: (D, D)) {
    let f = move || p.1.0;
    println!("move field {}", f());
    drop(f);
    println!("closure dropped");
}

fn move_through_index(p: (D, [D; 2])) {
    let f = move || p.1[0].0;
    println!("move through index {}", f());
    drop(f);
    println!("closure dropped");
}

fn move_drop_type_field(g: Guarded) {
    let f = move || g.inner.0;
    println!("drop type field {}", f());
    drop(f);
    println!("closure dropped");
}

fn move_drop_type_copy_field(g: Guarded) {
    let f = move || g.n;
    println!("drop type copy field {}", f());
    drop(f);
    println!("closure dropped");
}

fn move_drop_type_borrowed(g: Guarded) {
    let f = move || {
        let r = &g.inner;
        r.0
    };
    println!("drop type borrowed {}", f());
    drop(f);
    println!("closure dropped");
}

fn move_through_ref(r: &Pair) -> i32 {
    let f = move || r.a.0 + r.b.0;
    f()
}

// Fields are captured in the order of the fields, not of their uses,
// and so dropped in that order with the closure.
fn field_order(p: Pair) {
    let f = move || {
        let x = &p.b;
        let y = &p.a;
        x.0 + y.0
    };
    println!("field order {}", f());
    drop(f);
    println!("closure dropped");
}

fn binding_order(p: Pair, q: D, r: (D, D)) {
    let f = move || {
        let x = &r.1;
        let y = &q;
        let z = &p.b;
        let w = &r.0;
        let v = &p.a;
        x.0 + y.0 + z.0 + w.0 + v.0
    };
    println!("binding order {}", f());
    drop(f);
    println!("closure dropped");
}

fn nested_field_order(t: ((D, D), D)) {
    let f = move || {
        let a = &t.1;
        let b = &t.0.1;
        let c = &t.0.0;
        a.0 + b.0 + c.0
    };
    println!("nested field order {}", f());
    drop(f);
    println!("closure dropped");
}

fn whole_after_part(p: Pair) {
    let f = move || {
        let x = &p.b;
        let y = &p;
        x.0 + y.a.0
    };
    println!("whole after part {}", f());
    drop(f);
    println!("closure dropped");
}

// Indices are evaluated after the place they index.
fn index_order(a: [D; 2], n: D) {
    let f = move || a[n.index()].0;
    println!("index order {}", f());
    drop(f);
    println!("closure dropped");
}

// Changing what a raw pointer points to borrows the pointer shared: the
// closure is `Fn`, and the pointer need not be `mut`.
fn raw_pointer_write() {
    let mut n = 1;
    let p: *mut i32 = &mut n;
    let f = || unsafe { *p += 10 };
    fn twice(f: &impl Fn()) {
        f();
        f();
    }
    twice(&f);
    println!("raw pointer {}", n);
}

fn raw_pointer_field() {
    let mut pair = (1, 2);
    let p: *mut (i32, i32) = &mut pair;
    let f = || unsafe { (*p).1 = 20 };
    let g = &f;
    g();
    println!("raw pointer field {:?}", pair);
}

// Through a `&mut` the place itself is captured; behind a shared
// reference, what the last such reference points to.
fn through_mut_ref(p: &mut Pair) {
    let mut f = || p.b.bump();
    f();
    println!("through mut ref {} {}", p.a.0, p.b.0);
}

fn through_shared_then_mut(r: &&mut Pair) -> i32 {
    let f = || r.a.0 + r.b.0;
    f()
}

fn main() {
    main_more();
    println!("{:?}", copy_method_receiver((D(1), [D(2), D(3)])));
    println!("{:?}", copy_field_read((D(4), [D(5), D(6)])));
    println!("{}", copy_let((D(7), [D(8), D(9)])));
    println!("{}", copy_tuple_pattern((D(10), [(1, 2), (3, 4)])));
    println!("{}", copy_match((D(11), [C(5), C(6)])));
    println!("{}", copy_struct_receiver((D(12), [C(5), C(6)])));
    println!("{}", ref_method((D(13), [D(14), D(15)])));
    println!("{}", mut_method((D(16), [D(17), D(18)])));
    println!("{}", index_by_capture((D(19), [D(20), D(21), D(22)]), (2, D(23))));
    println!("{}", nested_index([[C(1), C(2)], [C(3), C(4)]], 1, 0));
    move_field((D(24), D(25)));
    move_through_index((D(26), [D(27), D(28)]));
    move_drop_type_field(Guarded { n: 29, inner: D(30) });
    move_drop_type_copy_field(Guarded { n: 31, inner: D(32) });
    move_drop_type_borrowed(Guarded { n: 33, inner: D(34) });
    let pair = Pair { a: D(35), b: D(36) };
    println!("through ref {}", move_through_ref(&pair));
    drop(pair);
    field_order(Pair { a: D(39), b: D(40) });
    binding_order(Pair { a: D(41), b: D(42) }, D(43), (D(44), D(45)));
    nested_field_order(((D(46), D(47)), D(48)));
    whole_after_part(Pair { a: D(49), b: D(50) });
    index_order([D(51), D(52)], D(53));
    raw_pointer_write();
    raw_pointer_field();
    let mut pair = Pair { a: D(55), b: D(56) };
    through_mut_ref(&mut pair);
    drop(pair);
    let mut pair = Pair { a: D(57), b: D(58) };
    println!("shared then mut {}", through_shared_then_mut(&&mut pair));
}

struct W([i32; 3], D);

impl Index<usize> for W {
    type Output = i32;
    fn index(&self, i: usize) -> &i32 {
        &self.0[i]
    }
}

impl IndexMut<usize> for W {
    fn index_mut(&mut self, i: usize) -> &mut i32 {
        &mut self.0[i]
    }
}

fn overloaded(w: W) -> i32 {
    let f = || w[1] + w[2];
    f()
}

fn overloaded_mut(mut w: W) -> i32 {
    let mut f = || w[0] += 5;
    f();
    f();
    w[0]
}

fn overloaded_move(w: W) {
    let f = move || w[0];
    println!("overloaded move {}", f());
    drop(f);
    println!("closure dropped");
}

fn assign_element(mut p: (D, [i32; 2])) -> [i32; 2] {
    let mut f = || p.1[0] = 9;
    f();
    p.1
}

fn compound_element(mut p: [i32; 2]) -> [i32; 2] {
    let mut f = || p[1] *= 3;
    f();
    f();
    p
}

fn borrow_element_mut(mut p: [(i32, i32); 2]) -> [(i32, i32); 2] {
    let mut f = || {
        let r = &mut p[1].0;
        *r += 1;
    };
    f();
    p
}

fn through_ref(r: &[D; 2]) -> i32 {
    let f = || r[0].0 + r[1].0;
    f()
}

fn through_mut_ref_array(r: &mut [i32; 2]) {
    let mut f = || r[0] += 100;
    f();
    println!("through mut ref {:?}", r);
}

fn slice_index(s: &[i32]) -> i32 {
    let f = move || s[0] + s[s.len() - 1];
    f()
}

fn nested(p: (D, [i32; 2])) -> i32 {
    let outer = || {
        let inner = || p.1[1].pow(2);
        inner() + 1
    };
    outer()
}

fn nested_move_index(p: (D, [D; 2])) {
    let outer = move || {
        let inner = || p.1[1].0;
        inner()
    };
    println!("nested move {}", outer());
    drop(outer);
    println!("closure dropped");
}

fn method_on_copy(p: [(u8, i64); 2]) -> (u32, i64) {
    let f = || (p[0].0.count_ones(), p[1].1.abs());
    f()
}

fn main_more() {
    println!("{}", overloaded(W([1, 2, 3], D(1))));
    println!("{}", overloaded_mut(W([1, 2, 3], D(2))));
    overloaded_move(W([4, 5, 6], D(3)));
    println!("{:?}", assign_element((D(4), [1, 2])));
    println!("{:?}", compound_element([1, 2]));
    println!("{:?}", borrow_element_mut([(1, 2), (3, 4)]));
    let a = [D(5), D(6)];
    println!("{}", through_ref(&a));
    let mut b = [1, 2];
    through_mut_ref_array(&mut b);
    println!("{}", slice_index(&[3, 4, 5]));
    println!("{}", nested((D(7), [3, 4])));
    nested_move_index((D(8), [D(9), D(10)]));
    println!("{:?}", method_on_copy([(7, -3), (1, -9)]));
}
