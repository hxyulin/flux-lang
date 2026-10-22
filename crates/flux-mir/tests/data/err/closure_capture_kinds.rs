// What closures capture, seen in the conflicts of their captures: the
// array an element is indexed in, by reference for a by-value use of a
// `Copy` part; a whole value of a type with a `Drop` impl moved for a
// field of it; the field order of their captures.

struct D(i32);

impl Drop for D {
    fn drop(&mut self) {}
}

struct Guarded {
    n: i32,
    inner: D,
}

impl Drop for Guarded {
    fn drop(&mut self) {}
}

fn copy_part_of_element() {
    let mut p = (D(0), [D(1), D(2)]);
    let c = || p.1[1].0.is_positive();
    p.1[0] = D(3);
    c();
}

fn copy_part_through_let() {
    let mut p = [(1, 2), (3, 4)];
    let c = || {
        let (a, _) = p[0];
        a
    };
    let r = &mut p[1];
    c();
    let _ = r;
}

fn element_method_mut() {
    let mut p = (D(0), [D(1), D(2)]);
    let mut c = || p.1[0].0 += 1;
    let r = &p.1;
    c();
    let _ = r;
}

fn move_whole_drop_value() {
    let g = Guarded { n: 1, inner: D(2) };
    let c = move || {
        let r = &g.inner;
        r.0
    };
    let n = g.n;
    c();
    let _ = n;
}

fn move_copy_field_of_drop_value() {
    let g = Guarded { n: 1, inner: D(2) };
    let c = move || g.n;
    let r = &g.inner;
    c();
    let _ = r;
}

fn raw_pointer_needs_no_mut() {
    let mut n = 1;
    let p: *mut i32 = &mut n;
    let c = || unsafe { *p = 2 };
    let q = &p;
    c();
    let _ = q;
}

fn main() {
    copy_part_of_element();
    copy_part_through_let();
    element_method_mut();
    move_whole_drop_value();
    move_copy_field_of_drop_value();
    raw_pointer_needs_no_mut();
}
