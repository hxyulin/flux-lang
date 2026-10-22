// Items in blocks: their types in annotations, their traits' methods,
// and their privacy, which is their module's.

trait Area {
    fn area(&self) -> i32;
}

fn shapes(side: i32) -> i32 {
    let s: Square = Square { side };
    struct Square {
        side: i32,
    }
    impl Area for Square {
        fn area(&self) -> i32 {
            self.side * self.side
        }
    }
    trait Perimeter {
        fn perimeter(&self) -> i32;
    }
    impl Perimeter for Square {
        fn perimeter(&self) -> i32 {
            4 * self.side
        }
    }
    s.area() + s.perimeter()
}

fn generic<T: Copy>(x: T) -> (T, u8) {
    fn plain<U>(u: U) -> U {
        u
    }
    type Byte = u8;
    let b: Byte = plain(7);
    (plain(x), b)
}

struct Counter;

impl Counter {
    fn count(&self) -> i32 {
        fn base() -> i32 {
            3
        }
        base() + 1
    }
}

fn main() {
    let a = shapes(2);
    let g = generic(true);
    let c = Counter.count();
}
