trait Read {
    fn read(self) -> i32;
}

struct Reader<'a>(&'a i32);
struct Wrapped<T>(T);

impl<'a> Read for Reader<'a> {
    fn read(self) -> i32 {
        *self.0 + 1
    }
}

impl<T: Read> Read for Wrapped<T> {
    fn read(self) -> i32 {
        self.0.read() + 2
    }
}

fn apply<T: Read>(reader: T) -> i32 {
    reader.read()
}

fn main() {
    let value = 39;
    println!("{}", apply(Wrapped(Reader(&value))));
    println!("{}", apply(Reader(&value)));
}
