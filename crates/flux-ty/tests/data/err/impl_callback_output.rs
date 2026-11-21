struct Once;
trait Run {
    fn run<F: FnMut()>(self, f: F);
}
impl Run for Once {
    fn run<F: FnMut() -> u8>(self, mut f: F) -> F::Output {
        f()
    }
}
fn main() {}
