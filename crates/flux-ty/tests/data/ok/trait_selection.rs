//! Trait method and impl selection that once went wrong: an associated
//! function on a generic struct's path, a bounded blanket impl next to a
//! concrete one, an inherent `&self` method against an impl for `&T`, and
//! an impl for `&S` whose method takes `self`.

mod case_01 {
    // `W::make(1)` for a trait fn implemented for `W<T>`: rustc infers `T`.
    struct W<T>(T);

    trait Make {
        fn make(x: i32) -> Self;
    }

    impl Make for W<i32> {
        fn make(x: i32) -> Self {
            W(x)
        }
    }

    pub fn run() {
        let _w: W<i32> = W::make(1);
    }
}

mod case_09 {
    // A blanket impl over an unimplemented trait and a concrete impl for `S`:
    // rustc selects the concrete impl; flux reports "type annotations needed"
    // for the method call and for `<S as Q>::q(&S)` and `Q::q(&S)` alike.
    struct S;

    trait Never {}

    trait Q {
        fn q(&self) -> i32;
    }

    impl<T: Never> Q for T {
        fn q(&self) -> i32 {
            1
        }
    }

    impl Q for S {
        fn q(&self) -> i32 {
            2
        }
    }

    pub fn run() {
        let _ = S.q();
        let _ = <S as Q>::q(&S);
        let _ = Q::q(&S);
    }
}

mod case_10 {
    // With `impl<T: Tr> Tr for &T`, `w.m()` on `w: &W<S>` must still pick the
    // inherent `W::m(&self)` (it matches the receiver by value at the first
    // autoderef step); rustc accepts this, flux picks `<&W<S> as Tr>::m`.
    struct S;

    struct W<T>(T);

    trait Tr {
        fn m(&self) -> i32;
    }

    impl Tr for S {
        fn m(&self) -> i32 {
            1
        }
    }

    impl<T: Tr> Tr for W<T> {
        fn m(&self) -> i32 {
            3
        }
    }

    impl<T: Tr> Tr for &T {
        fn m(&self) -> i32 {
            2
        }
    }

    impl<T> W<T> {
        fn m(&self) -> u8 {
            1
        }
    }

    fn names(w: &W<S>) -> u8 {
        w.m()
    }
}

mod case_13 {
    // `impl Tr for &S` with a by-value `self`, called as `r.get()` on `r: &S`:
    // rustc passes `r` itself and the program prints 1. flux autorefs the
    // receiver (MIR: `_6 = &_2; get::<&S>(copy _6)`, an `&&S` where `&S` is
    // expected), so the program prints garbage.
    unsafe extern "C" {
        fn printf(format: *const i8, ...) -> i32;
    }

    struct S {
        x: i32,
    }

    trait Tr {
        fn get(self) -> i32;
    }

    impl Tr for &S {
        fn get(self) -> i32 {
            self.x
        }
    }

    pub fn run() {
        let s = S { x: 1 };
        let r = &s;
        unsafe { printf(c"%d\n".as_ptr(), r.get()) };
    }
}

fn main() {}
