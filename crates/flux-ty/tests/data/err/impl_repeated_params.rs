// An inherent `impl` for `Pair<T, T>` is only for pairs whose two types
// are the same: its methods are not found on `Pair<i32, u8>` (E0599).

struct Pair<A, B>(A, B);

impl<T: Copy> Pair<T, T> {
    fn second(&self) -> T {
        self.1
    }
}

fn method() {
    let p = Pair(1i32, 2u8);
    let x: i32 = p.second();
}

fn nested(p: Pair<Option<i32>, Option<u8>>) {
    let y = p.second();
}

fn main() {}
