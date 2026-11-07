struct Wrap<T>(T);

trait Value {
    fn value(&self) -> i32;
}

impl Value for Wrap<i32> {
    fn value(&self) -> i32 {
        self.0 + 1
    }
}

impl Value for Wrap<u32> {
    fn value(&self) -> i32 {
        self.0 as i32 + 10
    }
}

#[inline(never)]
fn value<T: Value>(x: T) -> i32 {
    x.value()
}

#[inline(never)]
fn other() -> i32 {
    let f: fn(Wrap<i32>) -> i32 = value::<Wrap<i32>>;
    f(Wrap(30))
}

fn main() {
    let signed: fn(Wrap<i32>) -> i32 = value::<Wrap<i32>>;
    let unsigned: fn(Wrap<u32>) -> i32 = value::<Wrap<u32>>;
    println!(
        "{} {} {} {}",
        signed(Wrap(1)),
        unsigned(Wrap(2)),
        other(),
        value(Wrap(4u32))
    );
}
