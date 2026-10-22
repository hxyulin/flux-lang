struct Counter;

impl Counter {
    fn count(&self) {
        fn make() -> Self {
            Counter
        }
    }
}

fn generic<T>(t: T) {
    fn inner(v: T) {}
    struct Holder {
        value: T,
    }
}

fn main() {}
