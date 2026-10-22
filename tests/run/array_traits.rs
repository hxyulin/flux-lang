//! `Clone` and `PartialEq` of arrays, built into the compiler: derived
//! for structs with array fields, called directly and through generic
//! code.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}
#[derive(Clone, PartialEq)]
struct W {
    a: [u8; 2],
}
#[derive(Clone, PartialEq)]
struct N {
    v: i32,
}
fn same<T: PartialEq>(a: &T, b: &T) -> bool {
    a == b
}
fn main() {
    let a = [1, 2, 3];
    let b = a.clone();
    let w = W { a: [1, 2] };
    let v = w.clone();
    let ns = [N { v: 1 }, N { v: 2 }];
    let ms = ns.clone();
    let ks = [N { v: 1 }, N { v: 3 }];
    unsafe {
        printf(
            c"%d %d %d %d %d %d %d\n".as_ptr(),
            (a == b) as i32,
            (a != [1, 2, 4]) as i32,
            (w == v) as i32,
            (ns == ms) as i32,
            (ns == ks) as i32,
            same(&ns, &ks) as i32,
            ms[1].v,
        )
    };
}
