unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct F {
    v: i32,
}

fn set(f: &mut F) {
    f.v = 1;
}

fn shared_to_unique() {
    let mut f = F { v: 0 };
    let m = &mut f;
    let r = &m;
    set(r);
}

fn deref_then_unsize() {
    let a = [1, 2, 3];
    let r = &a;
    let s: &[i32] = &r;
    let _ = s;
}

fn not_the_pointee() {
    let f = F { v: 0 };
    let r = &f;
    let x: &i32 = &r;
    let _ = x;
}

fn function_item() {
    unsafe {
        printf(c"%p".as_ptr(), set);
    }
}

fn main() {}
