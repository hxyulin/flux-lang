use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(PartialEq, Eq, Hash)]
struct Key(u32);
struct Digest(u64);
impl Hasher for Digest {
    fn finish(&self) -> u64 { self.0 }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes { self.0 = self.0.wrapping_mul(31).wrapping_add(byte as u64); }
    }
}
fn digest<T: Hash>(value: &T) -> u64 {
    let mut state = Digest(0);
    value.hash(&mut state);
    state.finish()
}
fn main() {
    let first = Arc::new(Key(17));
    let equal = Arc::new(Key(17));
    let different = Arc::new(Key(23));
    assert!(first == equal);
    assert!(first != different);
    assert_eq!(digest(&first), digest(&Key(17)));
    assert_eq!(digest(&first), digest(&equal));
    let a: Arc<[u32]> = Arc::new([1u32, 2, 3]);
    let b: Arc<[u32]> = Arc::new([1u32, 2, 3]);
    assert!(a == b);
    assert_eq!(digest(&a), digest(&b));
    println!("arc value traits {} {}", digest(&first), a.len());
}
