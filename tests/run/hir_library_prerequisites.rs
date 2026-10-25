use std::cell::Cell;
use std::sync::Arc;

struct NoDefault;
struct Owner<'a>(&'a Cell<usize>, usize);
impl Drop for Owner<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
fn main() {
    let empty: Box<[NoDefault]> = Default::default();
    assert!(empty.is_empty());
    let mut cloned: Box<[String]> = Box::from(&[String::from("one"), String::from("two")][..]);
    for value in &mut cloned { value.push('!'); }
    assert_eq!((&cloned).into_iter().map(|value| value.len()).sum::<usize>(), 8);
    let lengths: Vec<usize> = cloned.into_iter().map(|value| value.len()).collect();
    assert_eq!(lengths, [4, 4]);
    let string = String::from("héllo");
    let text = string.into_boxed_str();
    assert_eq!(&*text, "héllo");
    let converted: Box<str> = String::from("converted").into();
    assert_eq!(&*converted, "converted");
    let empty_text: Box<str> = Default::default();
    assert!(empty_text.is_empty());
    let mut option = Some(vec![3u32, 5].into_boxed_slice());
    option.as_deref_mut().unwrap()[0] = 7;
    assert_eq!(option.as_deref().unwrap(), [7, 5]);
    let none: Option<Box<[u32]>> = None;
    assert!(none.as_deref().is_none());
    let range = String::from("a")..=String::from("z");
    assert_eq!(range.start(), "a"); assert_eq!(range.end(), "z");
    assert_eq!(format!("{:?}", Arc::new([1u32, 2])), "[1, 2]");
    let drops = Cell::new(0);
    {
        let owners: Box<[_]> = vec![Owner(&drops, 1), Owner(&drops, 2), Owner(&drops, 3)].into_boxed_slice();
        let mut iter = IntoIterator::into_iter(owners);
        assert_eq!(iter.next().unwrap().1, 1);
        assert_eq!(drops.get(), 1);
    }
    assert_eq!(drops.get(), 3);
    for byte in 0u16..=255 {
        let bytes = [byte as u8];
        let ascii: Vec<_> = core::ascii::escape_default(bytes[0]).collect();
        assert_eq!(ascii, bytes.escape_ascii().collect::<Vec<_>>());
        print!("{};", bytes.escape_ascii());
    }
    println!();
    for value in (0..=0x10ffff).step_by(997) {
        if let Some(c) = char::from_u32(value) {
            let mut escaped = c.escape_default();
            let len = escaped.len();
            let text: String = escaped.clone().collect();
            assert_eq!(text.len(), len);
            assert_eq!(format!("{}", escaped), text);
            escaped.next();
            assert_eq!(escaped.len(), len - 1);
            print!("{text};");
        }
    }
    println!();
    println!("hir libraries {} {} {}", text.len(), lengths.len(), drops.get());
}
