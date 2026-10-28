use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Item(usize);
impl Drop for Item {
    fn drop(&mut self) { DROPS.fetch_add(1, Ordering::Relaxed); }
}
fn main() {
    let mut queue = VecDeque::with_capacity(4);
    for n in 0..4 { queue.push_back(Item(n)); }
    assert_eq!(queue.pop_front().unwrap().0, 0);
    assert_eq!(queue.pop_front().unwrap().0, 1);
    queue.push_back(Item(4));
    queue.push_back(Item(5));
    queue.reserve(100);
    assert!(queue.capacity() >= queue.len() + 100);
    let capacity = queue.capacity();
    queue.reserve(0);
    assert_eq!(queue.capacity(), capacity);
    for expected in 2..6 { assert_eq!(queue.pop_front().unwrap().0, expected); }
    assert!(queue.is_empty());
    assert_eq!(DROPS.load(Ordering::Relaxed), 6);
    let mut empty: VecDeque<u8> = VecDeque::new();
    empty.reserve(3);
    empty.push_front(7);
    assert_eq!(empty.pop_back(), Some(7));
    for text in ["", "a/../b/λ", "/tmp/🙂"] {
        let path = PathBuf::from(text).into_boxed_path();
        assert_eq!(path.to_str(), Some(text));
        let os = OsString::from(text).into_boxed_os_str();
        assert_eq!(os.to_str(), Some(text));
        println!("{:?} {:?}", path, os);
    }
    fn error<E: std::error::Error>(value: E) { assert!(value.source().is_none()); println!("{}", value); }
    error(std::fmt::Error);
    println!("drops {}", DROPS.load(Ordering::Relaxed));
}
