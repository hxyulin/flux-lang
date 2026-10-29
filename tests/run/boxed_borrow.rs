use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
trait Read {
    fn read(&self) -> u32;
}
impl Read for u32 {
    fn read(&self) -> u32 {
        *self
    }
}
fn main() {
    let mut map = HashMap::<Box<[u8]>, String>::new();
    map.insert(Box::from([3u8, 5, 7]), String::from("value"));
    assert_eq!(map.get(&[3u8, 5, 7][..]).unwrap(), "value");
    assert!(map.get(&[3u8, 5][..]).is_none());
    let mut bytes: Box<[u8]> = Box::from([11u8, 13, 17]);
    let address = bytes.as_ptr();
    let shared: &[u8] = Borrow::<[u8]>::borrow(&bytes);
    assert_eq!(shared.as_ptr(), address);
    let mutable: &mut [u8] = BorrowMut::<[u8]>::borrow_mut(&mut bytes);
    mutable[1] = 19;
    assert_eq!(bytes[1], 19);
    let mut text: Box<str> = Box::from("ASCII Ω");
    let text_address = text.as_ptr();
    let shared_text: &str = Borrow::<str>::borrow(&text);
    assert_eq!(shared_text.as_ptr(), text_address);
    BorrowMut::<str>::borrow_mut(&mut text).make_ascii_lowercase();
    assert_eq!(&*text, "ascii Ω");
    let mut value = Box::new(23u32);
    *BorrowMut::<u32>::borrow_mut(&mut value) = 29;
    let mut object: Box<dyn Read> = Box::new(31u32);
    assert_eq!(Borrow::<dyn Read>::borrow(&object).read(), 31);
    assert_eq!(BorrowMut::<dyn Read>::borrow_mut(&mut object).read(), 31);
    println!(
        "boxed borrow {} {} {}",
        bytes[1],
        text,
        Borrow::<u32>::borrow(&value)
    );
}
