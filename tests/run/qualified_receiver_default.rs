trait Encoding {
    const WORD: usize;
}

struct Container<T = u32>(T);

impl<T> Encoding for Container<T> {
    const WORD: usize = core::mem::size_of::<T>();
}

impl<T> Container<T> {
    fn new(value: T) -> Self {
        Self(value)
    }
}

const WORD: usize = <Container as Encoding>::WORD;

fn main() {
    let default = <Container>::new(7);
    let inferred = Container::new(9u8);
    println!("{WORD} {} {}", default.0, inferred.0);
}
