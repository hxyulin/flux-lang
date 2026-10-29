struct S<'a> {
    r: &'a mut i32,
}

struct W<'a> {
    s: S<'a>,
}

// Overwriting `s` leaves what `s.r` pointed to borrowed, not `s` itself.
fn struct_field() -> i32 {
    let (mut a, mut b) = (1, 2);
    let mut s = S { r: &mut a };
    let x = &mut *s.r;
    s = S { r: &mut b };
    *x += 1;
    *s.r + a
}

// The dereference may be more than one projection past the written place.
fn nested_field() -> i32 {
    let (mut a, mut b) = (1, 2);
    let mut w = W { s: S { r: &mut a } };
    let x = &mut *w.s.r;
    w = W { s: S { r: &mut b } };
    *x += 10;
    *w.s.r + a
}

fn main() {
    let _ = struct_field();
    let _ = nested_field();
}
