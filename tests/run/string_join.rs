extern crate alloc;

use std::borrow::{Borrow, BorrowMut};

struct Text(String);

impl Borrow<str> for Text {
    fn borrow(&self) -> &str {
        &self.0
    }
}

fn borrowed<T: core::borrow::Borrow<str>>(value: &T) -> &str {
    value.borrow()
}

fn main() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(empty.join("ignored"), "");
    assert_eq!(["é😀"].join("ignored"), "é😀");
    assert_eq!(vec![String::from("single")].join("ignored"), "single");
    assert_eq!(["", "", ""].join("λ"), "λλ");
    assert_eq!(["a", "b", "c"].join(""), "abc");
    let strings = vec![String::from("中"), String::from("é"), String::from("😀")];
    assert_eq!(strings.join(" / "), "中 / é / 😀");
    let custom = [Text(String::from("left")), Text(String::from("右"))];
    assert_eq!(custom.join(" → "), "left → 右");
    assert_eq!(borrowed(&custom[1]), "右");
    let mut owned = String::from("abc");
    let text: &mut str = <String as BorrowMut<str>>::borrow_mut(&mut owned);
    assert_eq!(text, "abc");
    let via_alloc: &str = <String as alloc::borrow::Borrow<str>>::borrow(&owned);
    assert_eq!(via_alloc, "abc");
    let slice: &str = "borrowed";
    assert_eq!(<&str as Borrow<str>>::borrow(&slice), "borrowed");
    let number = 42;
    assert_eq!(*<i32 as Borrow<i32>>::borrow(&number), 42);
    let mut mutable = 1;
    *<i32 as BorrowMut<i32>>::borrow_mut(&mut mutable) = 7;
    assert_eq!(mutable, 7);
    struct Digest(u64);
    impl core::hash::Hasher for Digest {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.0 = self.0.wrapping_mul(31).wrapping_add(*byte as u64);
            }
        }
    }
    let mut string_hash = Digest(0);
    let mut str_hash = Digest(0);
    core::hash::Hash::hash(&owned, &mut string_hash);
    core::hash::Hash::hash(owned.as_str(), &mut str_hash);
    assert_eq!(string_hash.0, str_hash.0);
    let values = vec![1, 2, 3];
    let shared = &values;
    assert!(shared.iter().any(|n| *n == 2));
    assert!(!shared.iter().all(|n| *n < 3));
    println!("{}; {}; {}", strings.join(" / "), custom.join(" → "), owned);
}
