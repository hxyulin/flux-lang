use std::cell::Cell;
use std::sync::OnceLock;

struct Payload<'a> {
    drops: &'a Cell<usize>,
    text: String,
}
impl<'a> Drop for Payload<'a> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

fn main() {
    let number: OnceLock<usize> = OnceLock::new();
    assert!(number.get().is_none());
    assert_eq!(*number.get_or_init(|| 41), 41);
    assert_eq!(*number.get_or_init(|| panic!("initializer rerun")), 41);
    let drops = Cell::new(0usize);
    {
        let empty: OnceLock<Payload<'_>> = OnceLock::default();
        assert!(empty.get().is_none());
        let lock: OnceLock<Payload<'_>> = OnceLock::default();
        let text = String::from("owned");
        let first = lock.get_or_init(|| {
            assert!(lock.get().is_none());
            Payload {
                drops: &drops,
                text,
            }
        });
        assert_eq!(first.text.as_str(), "owned");
        let captured = Payload {
            drops: &drops,
            text: String::from("unused"),
        };
        assert_eq!(lock.get_or_init(|| captured).text.as_str(), "owned");
        assert_eq!(drops.get(), 1);
        assert_eq!(lock.get().unwrap().text.as_str(), "owned");
    }
    assert_eq!(drops.get(), 2);
    let x = 73u32;
    let borrowed = OnceLock::new();
    assert_eq!(**borrowed.get_or_init(|| &x), 73);
    let buckets: [OnceLock<Box<[usize]>>; 3] = std::array::from_fn(|_| OnceLock::new());
    let bucket = buckets[1].get_or_init(|| vec![7usize, 11].into_boxed_slice());
    assert_eq!(bucket[1], 11);
    assert_eq!(buckets[1].get().map(|b| b.len()), Some(2));
    assert!(buckets[0].get().is_none());
    println!("once-lock 41 owned 2 73 11");
}
