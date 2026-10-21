// Functions with the same name in different places: associated functions
// of two types, a method and a free function, and a method called `main`
// besides the real one. Each needs its own symbol.
unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct A(u32);
struct B(u64);

impl A {
    fn new() -> Self {
        A(1)
    }

    fn main(&self) -> u32 {
        self.0 + 40
    }
}

impl B {
    fn new() -> Self {
        B(2)
    }

    fn add(&self, other: &B) -> B {
        B(self.0 + other.0)
    }
}

fn add(a: i64, b: i64) -> i64 {
    a + b
}

fn main() {
    let a = A::new();
    let b = B::new().add(&B::new());
    unsafe {
        printf(c"%u %llu %u %lld\n".as_ptr(), a.0, b.0, a.main(), add(5, 6));
    }
}
