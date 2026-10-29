// Overwriting a struct leaves loans of what its references pointed to
// alone, however deep the reference is.

struct S<'a> {
    r: &'a mut i32,
}

struct W<'a> {
    s: S<'a>,
}

fn struct_field() -> i32 {
    let (mut a, mut b) = (1, 2);
    let mut s = S { r: &mut a };
    let x = &mut *s.r;
    s = S { r: &mut b };
    *x += 1;
    *s.r += 5;
    a * 10 + b
}

fn nested_field() -> i32 {
    let (mut a, mut b) = (1, 2);
    let mut w = W { s: S { r: &mut a } };
    let x = &mut *w.s.r;
    w = W { s: S { r: &mut b } };
    *x += 10;
    *w.s.r += 20;
    a * 100 + b
}

fn main() {
    println!("{}", struct_field());
    println!("{}", nested_field());
}
