//! Trait errors that once went wrong: a method of another instance, an
//! integer receiver's fallback, a trait path with the wrong receiver, a
//! generic trait's single impl, an impl stricter than its trait, and an
//! unsized `Self` in provided methods.

mod case_02 {
    // rustc names the struct generically (`W<T>`) when an impl exists only
    // for another instance.
    struct S;

    struct W<T>(T);

    trait OnlyS {
        fn only(&self) {}
    }

    impl OnlyS for W<S> {}

    pub fn run() {
        W(1i32).only();
    }
}

mod case_03 {
    // With impls for two integer types, the literal falls back to `i32` and
    // rustc reports E0277 `i32: Int` at the method name.
    trait Int {
        fn int(self);
    }

    impl Int for u8 {
        fn int(self) {}
    }

    impl Int for i64 {
        fn int(self) {}
    }

    pub fn run() {
        1.int();
    }
}

mod case_04 {
    // `Tr::m(s)` with a receiver of the wrong type: rustc reports only the
    // E0308 on the argument; flux adds E0790 on the call.
    struct S;

    trait Tr {
        fn m(&self);

        fn mutate(&mut self);
    }

    impl Tr for S {
        fn m(&self) {}

        fn mutate(&mut self) {}
    }

    pub fn run() {
        let s = S;
        Tr::mutate(&s);
        Tr::m(s);
    }
}

mod case_07 {
    // The only impl of `Conv<_>` for `S` is `Conv<bool>`: rustc selects it and
    // reports E0308 on the call (expected `i64`, found `bool`); flux reports
    // E0599 "exists ... but its trait bounds were not satisfied".
    struct S;

    trait Conv<T> {
        fn conv(&self) -> T;
    }

    impl Conv<bool> for S {
        fn conv(&self) -> bool {
            true
        }
    }

    pub fn run() {
        let _x: i64 = S.conv();
    }
}

mod case_08 {
    // An impl method with a bound its trait method lacks: rustc reports E0276
    // "impl has stricter requirements than trait" at `Other`; flux accepts it.
    struct S;

    trait Other {}

    trait Tr {
        fn g<U>(&self, u: U);
    }

    impl Tr for S {
        fn g<U: Other>(&self, _u: U) {}
    }
}

mod case_12 {
    // In a provided method `Self` may be unsized. rustc reports E0277 "the size
    // for values of type `Self` cannot be known at compilation time" at 9:18,
    // 13:9 (calls returning `Self`) and 23:14 (a method that needs
    // `Self: Sized`); flux accepts the program.
    trait Tr {
        fn make() -> Self;

        fn a(&self) {
            let _x = Self::make();
        }

        fn b(&self) {
            Self::make();
        }
    }

    trait Tr2 {
        fn sized(&self)
        where
            Self: Sized;

        fn c(&self) {
            self.sized();
        }
    }
}

fn main() {}
