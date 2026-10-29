// An inherent `impl` for `Pair<T, T>` applies when both types are the
// same, or still may be.

struct Pair<A, B>(A, B);

impl<T: Copy> Pair<T, T> {
    fn second(&self) -> T {
        self.1
    }
}

fn main() {
    let same = Pair(1i32, 2i32).second();
    let unsuffixed = Pair(1, 2).second();
    let one_known = Pair(1, 2i64).second();
    let options = Pair(Some(1u16), None).second();
    let floats = Pair(1.5, 2.5f32).second();
    println!("{same} {unsuffixed} {one_known} {options:?} {floats}");
}
