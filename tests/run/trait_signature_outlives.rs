trait Read<T: 'static> {
    fn read(self, value: T) -> i32;
}

struct Reader;

impl Read<i32> for Reader {
    fn read(self, value: i32) -> i32 {
        value + 7
    }
}

fn invoke<T: 'static, N: Read<T>>(node: N, value: T) -> i32 {
    N::read(node, value)
}

fn main() {
    println!("{}", invoke(Reader, 35));
}
