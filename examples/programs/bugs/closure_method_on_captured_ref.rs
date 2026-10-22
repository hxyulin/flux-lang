//! Miscompilation: a non-`move` closure that calls a `&self` method on a
//! captured shared reference (`let r = &s; || r.get()`) is lowered to a
//! body that starts with `unreachable` (see `flux mir`): the method call
//! is never reached and the program traps (SIGTRAP, exit 133).
//! rustc prints 7. `move || r.get()`, `|| (*r).get()`, `|| S::get(r)`
//! and `|| r.v` all work; `|| slice.len()` on a captured `&[T]` traps too.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct S {
    v: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.v
    }
}

fn main() {
    let s = S { v: 7 };
    let r = &s;
    let f = || r.get();
    unsafe {
        printf(c"%d\n".as_ptr(), f());
    }
}
