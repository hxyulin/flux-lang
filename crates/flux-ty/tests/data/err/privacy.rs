// Private fields and methods used from outside their module. rustc reports
// these during type checking and stops before its privacy pass, so
// private fields in literals and patterns are in `private_fields.rs`.

mod m {
    pub struct S {
        a: i32,
        b: i32,
        pub c: i32,
    }

    pub struct T(i32, pub i32);

    pub struct Knob {
        pub go: i32,
        hidden: i32,
    }

    struct P;

    pub struct Leaky {
        pub p: P,
    }

    pub fn make() -> S {
        S { a: 1, b: 2, c: 3 }
    }

    pub fn make_t() -> T {
        T(1, 2)
    }

    pub fn knob() -> Knob {
        Knob { go: 1, hidden: 2 }
    }

    pub fn leak() -> P {
        P
    }

    pub fn leaky() -> Leaky {
        Leaky { p: P }
    }

    impl S {
        fn secret(&self) -> i32 {
            self.a + self.b
        }

        pub fn open(&self) -> i32 {
            self.secret()
        }

        fn hidden_new() -> S {
            make()
        }

        pub fn new() -> S {
            S::hidden_new()
        }
    }

    impl Knob {
        fn go(&self) -> i32 {
            self.go + self.hidden
        }

        pub fn total(&self) -> i32 {
            self.go()
        }
    }

    pub mod child {
        // A child module sees its parent's private fields.
        pub fn peek(s: &super::S) -> i32 {
            s.a + s.b
        }
    }
}

fn main() {
    let mut s = m::make();
    let t = m::make_t();
    let n = m::knob();
    let _ok = s.c + t.1 + n.go + s.open() + m::S::new().c + n.total() + m::child::peek(&s);
    let _a = s.a;
    s.b = 5;
    let _b = t.0;
    let _g = s.secret();
    let _i = n.go();
}
