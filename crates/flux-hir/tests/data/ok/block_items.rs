// Items declared in blocks: visible throughout their block, before
// their declaration too, and in the blocks inside it; an inner item
// shadows an outer one in its block only.

fn helper() -> i32 {
    1
}

mod m {
    pub fn far() -> i32 {
        2
    }
}

fn main() {
    let s = Square { side: early() };
    fn early() -> i32 {
        3
    }
    struct Square {
        side: i32,
    }
    impl Square {
        fn area(&self) -> i32 {
            self.side * self.side * SCALE
        }
    }
    const SCALE: i32 = 2;
    static mut CALLS: u32 = 0;
    type Count = u32;
    use m::far;
    enum Kind {
        A,
        B(i32),
    }
    let k = Kind::B(far());
    let inner = {
        fn helper() -> i32 {
            10
        }
        mod nested {
            pub fn deep() -> i32 {
                self::super_fn()
            }
            fn super_fn() -> i32 {
                4
            }
        }
        helper() + nested::deep()
    };
    let outer = helper();
    let a = s.area();
    let c: Count = unsafe { CALLS };
}
