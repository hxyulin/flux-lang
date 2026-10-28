use std::collections::VecDeque;
use std::error::Error;
use std::sync::Arc;

const EMPTY: String = String::new();
const SIGNED: f64 = 0.0f64.copysign(-1.0);
const RECIP: f32 = 4.0f32.recip();

#[derive(Debug)]
struct Once { value: Option<u32> }
impl Iterator for Once {
    type Item = u32;
    fn next(&mut self) -> Option<u32> { self.value.take() }
}
#[derive(Debug)]
struct Failure;
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("failure") }
}
impl Error for Failure {}

struct Payload { drops: Arc<std::sync::atomic::AtomicUsize>, value: usize }
impl Drop for Payload {
    fn drop(&mut self) { self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst); }
}
fn main() {
    assert!(EMPTY.is_empty());
    assert_eq!(SIGNED.to_bits(), (-0.0f64).to_bits());
    assert_eq!(RECIP, 0.25);
    for bits in [0u64, 1, 0x7ff0000000000000, 0x7ff8000000001234] {
        for sign in [0u64, 1u64 << 63] {
            assert_eq!(f64::from_bits(bits).copysign(f64::from_bits(sign)).to_bits(), bits | sign);
        }
    }
    let error = String::from_utf8(vec![0xff]).unwrap_err();
    assert_eq!(error.to_string(), error.utf8_error().to_string());
    assert!(error.source().is_none());
    #[allow(deprecated)]
    { assert_eq!(Failure.description(), "description() is deprecated; use Display"); }
    let mut value = Box::new(3u32);
    let borrowed: &mut u32 = value.as_mut();
    *borrowed = 7;
    let borrowed: &u32 = value.as_ref();
    assert_eq!(*borrowed, 7);
    let mut fused = Once { value: Some(9) }.fuse();
    println!("{:?}", fused);
    assert_eq!(fused.next(), Some(9));
    assert_eq!(fused.next(), None);
    println!("{:?}", fused);
    let mut queue = VecDeque::with_capacity(7);
    assert!(queue.capacity() >= 7);
    for value in 0..32 { queue.push_back(value); }
    for value in 0..24 { assert_eq!(queue.pop_front(), Some(value)); }
    for value in 32..48 { queue.push_back(value); }
    assert_eq!(queue.into_iter().sum::<u32>(), (24..48).sum());
    let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let shared: Arc<Payload> = Arc::from(Box::new(Payload { drops: drops.clone(), value: 42 }));
    assert_eq!(shared.value, 42);
    let copy = shared.clone();
    drop(shared);
    assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(copy);
    let values: Box<[Payload]> = vec![Payload { drops: drops.clone(), value: 3 }, Payload { drops: drops.clone(), value: 5 }].into_boxed_slice();
    let shared: Arc<[Payload]> = Arc::from(values);
    assert_eq!(shared[0].value + shared[1].value, 8);
    drop(shared);
    assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 3);
    let text: Arc<str> = Arc::from(String::from("héλ").into_boxed_str());
    assert_eq!(&*text, "héλ");
    let empty: Arc<str> = Arc::from(String::new().into_boxed_str());
    assert!(empty.is_empty());
    println!("remaining APIs {}", drops.load(std::sync::atomic::Ordering::SeqCst));
}
