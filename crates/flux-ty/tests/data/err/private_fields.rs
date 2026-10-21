// Private fields in struct literals and patterns, and private types
// reached through public functions and fields.

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
    let s = m::make();
    let t = m::make_t();
    let n = m::knob();
    let _ok = s.c + t.1 + n.go + s.open() + m::S::new().c + n.total() + m::child::peek(&s);
    let _c = m::S { a: 1, b: 2, c: 3 };
    let _d = m::S { a: 1, ..m::make() };
    let a = 1;
    let _e = m::Knob { go: 1, hidden: a };
    let _f = m::T { 0: 1, 1: 2 };
    let m::S { a: _x, c: _y, .. } = m::make();
    let m::S { a, b, c } = m::make();
    let m::S { .. } = m::make();
    m::leak();
    let _k = m::leak();
    let l = m::leaky();
    let _p = l.p;
    let _sum = a + b + c;
}
