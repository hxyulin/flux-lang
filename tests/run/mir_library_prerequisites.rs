use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

struct Owned<'a>(&'a Cell<usize>, usize);
impl Drop for Owned<'_> { fn drop(&mut self) { self.0.set(self.0.get() + 1); } }
#[repr(align(128))]
struct Aligned(u8);
fn view<T: AsRef<[u32]>>(value: &T) -> &[u32] { value.as_ref() }
fn main() {
    let drops = Cell::new(0);
    let mut values = Vec::new();
    for i in 0..17 { values.push(Owned(&drops, i)); }
    let mut owner: Rc<[Owned<'_>]> = values.into();
    assert_eq!(owner.len(), 17);
    let other = owner.clone();
    assert!(Rc::get_mut(&mut owner).is_none());
    assert!(Rc::ptr_eq(&owner, &other));
    drop(other);
    Rc::get_mut(&mut owner).unwrap()[3].1 = 91;
    assert_eq!(owner[3].1, 91);
    drop(owner);
    assert_eq!(drops.get(), 17);
    let empty: Rc<[u8]> = Rc::default();
    assert!(empty.is_empty());
    let zst: Rc<[()]> = vec![(); 23].into();
    assert_eq!(zst.len(), 23);
    let aligned: Rc<[Aligned]> = vec![Aligned(7), Aligned(9)].into();
    assert_eq!(aligned[0].0 + aligned[1].0, 16);
    assert_eq!(aligned.as_ptr() as usize % 128, 0);
    let bytes: Rc<[u8]> = Rc::from(&b"source"[..]);
    assert_eq!(bytes.as_ref(), b"source");
    let values = vec![7u32, 11, 29];
    assert_eq!(view(&values).as_ptr(), values.as_ptr());
    for limit in [0usize, 1, 2, 7, usize::MAX] {
        println!("replace {} {} {}", limit, "éλéλ".replacen('é', "Q", limit), String::from("ab").replacen("", "λ", limit));
    }
    let called = Cell::new(0);
    let unchanged = "abc".replacen(|_: char| { called.set(called.get() + 1); true }, "x", 0);
    assert_eq!(unchanged, "abc");
    assert_eq!(called.get(), 0);
    let mut map = BTreeMap::new();
    for i in 0..211 { let key = i * 73 % 211; map.insert(key, key); }
    let mut iter = map.values_mut();
    assert_eq!(iter.len(), 211);
    let first = iter.next().unwrap();
    let last = iter.next_back().unwrap();
    *first = 500; *last = 900;
    for (i, value) in iter.enumerate() { assert_eq!(*value, i + 1); *value += 1000; }
    assert_eq!(map[&0], 500);
    assert_eq!(map[&210], 900);
    let mut text = BTreeMap::new();
    text.insert(String::from("key"), 17);
    assert_eq!(text["key"], 17);
    println!("ownership {} {} {}", drops.get(), map[&100], bytes.len());
}
