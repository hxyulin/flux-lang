fn require_send(_: impl Send) {}

trait ParallelSlice<T: Sync>: AsRef<[T]> {
    fn prove(&self) {
        let slice = self.as_ref();
        require_send(async move { slice.len() });
    }
}

impl<T: Sync> ParallelSlice<T> for [T] {}

fn main() {
    [1, 2].as_slice().prove();
}
