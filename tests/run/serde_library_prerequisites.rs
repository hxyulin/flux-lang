use std::cell::Cell;
use std::collections::{BinaryHeap, LinkedList};
use std::ffi::{CStr, CString};
struct Owned<'a> {
    value: u32,
    drops: &'a Cell<u32>,
}
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn main() {
    let mut heap = BinaryHeap::from(vec![3, 1, 7, 7, 2, 10, 4]);
    assert_eq!(heap.peek(), Some(&10));
    heap.push(18);
    assert_eq!(heap.pop(), Some(18));
    let mut other = BinaryHeap::from([6, 5]);
    heap.append(&mut other);
    assert!(other.is_empty());
    assert_eq!(heap.clone().into_sorted_vec(), [1, 2, 3, 4, 5, 6, 7, 7, 10]);
    let mut previous = i32::MAX;
    while let Some(n) = heap.pop() {
        assert!(n <= previous);
        previous = n;
    }
    let heap: BinaryHeap<i32> = (0..80).rev().collect();
    let cap = heap.capacity();
    let values = heap.into_vec();
    assert_eq!(values.capacity(), cap);
    assert_eq!(values.len(), 80);
    let drops = Cell::new(0);
    {
        let mut list = LinkedList::new();
        for value in 0..80 {
            list.push_back(Owned {
                value,
                drops: &drops,
            });
        }
        assert_eq!(list.pop_front().unwrap().value, 0);
        assert_eq!(list.pop_back().unwrap().value, 79);
        list.front_mut().unwrap().value = 100;
        list.back_mut().unwrap().value = 200;
        let mut second = LinkedList::new();
        second.push_front(Owned {
            value: 300,
            drops: &drops,
        });
        list.append(&mut second);
        assert!(second.is_empty());
        let mut iter = list.iter_mut();
        let a = iter.next().unwrap();
        let b = iter.next_back().unwrap();
        a.value += 1;
        b.value += 1;
        assert_eq!(a.value, 101);
        assert_eq!(b.value, 301);
        assert_eq!(iter.len(), 77);
        drop(iter);
        assert_eq!(list.iter().map(|n| n.value).sum::<u32>(), 3604);
        let mut owned = list.into_iter();
        assert_eq!(owned.next().unwrap().value, 101);
        assert_eq!(owned.next_back().unwrap().value, 301);
        assert_eq!(owned.len(), 77);
    }
    assert_eq!(drops.get(), 81);
    let mut list: LinkedList<_> = [1, 2, 3].into_iter().collect();
    assert_eq!(list.iter().copied().collect::<Vec<_>>(), [1, 2, 3]);
    list.clear();
    list.push_back(9);
    assert_eq!(list.pop_back(), Some(9));
    let text = CString::new("aé中").unwrap();
    assert_eq!(text.as_bytes(), "aé中".as_bytes());
    assert_eq!(text.to_str().unwrap(), "aé中");
    assert_eq!(text.as_bytes_with_nul().last(), Some(&0));
    let boxed = text.clone().into_boxed_c_str();
    assert_eq!(boxed.to_bytes(), text.as_bytes());
    let restored = CString::from(boxed);
    assert_eq!(restored, text);
    let pointer = text.into_raw();
    let text = unsafe { CString::from_raw(pointer) };
    assert_eq!(text.into_string().unwrap(), "aé中");
    let error = CString::new(b"a\0b".as_slice()).unwrap_err();
    assert_eq!(error.nul_position(), 1);
    assert_eq!(error.into_vec(), b"a\0b");
    let interior = CString::from_vec_with_nul(vec![1, 0, 2, 0]).unwrap_err();
    let absent = CString::from_vec_with_nul(vec![1, 2]).unwrap_err();
    println!("C errors {interior:?} {interior} {absent:?} {absent}");
    assert_eq!(interior.into_bytes(), [1, 0, 2, 0]);
    assert_eq!(absent.as_bytes(), [1, 2]);
    let empty = CString::default();
    assert_eq!(empty.as_bytes_with_nul(), [0]);
    let mut map = std::collections::HashMap::new();
    map.insert(CString::new("lookup").unwrap(), 31);
    assert_eq!(map.get(c"lookup"), Some(&31));
    let c: CString = c"hello".to_owned();
    assert_eq!(c.as_c_str(), c"hello");
    let c: &CStr = &c;
    assert_eq!(c.to_bytes(), b"hello");
    let invalid = CString::new(vec![255, 1])
        .unwrap()
        .into_string()
        .unwrap_err();
    assert_eq!(invalid.utf8_error().valid_up_to(), 0);
    assert_eq!(invalid.into_cstring().into_bytes(), [255, 1]);
    println!(
        "serde APIs {} {} {}",
        values.len(),
        drops.get(),
        restored.to_str().unwrap()
    );
}
