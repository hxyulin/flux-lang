use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
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
    let parts = ["aé", "", "中"];
    let joined = parts.concat();
    assert_eq!(joined, "aé中");
    assert_eq!(parts.join("|"), "aé||中");
    let empty: [&str; 0] = [];
    assert_eq!(empty.concat(), "");
    let strings = [String::from("one"), String::from("two")];
    assert_eq!(strings.concat(), "onetwo");
    let slices: [&[u32]; 3] = [&[1, 2], &[], &[3]];
    assert_eq!(slices.concat(), [1, 2, 3]);
    let drops = Cell::new(0);
    {
        let values = vec![
            Owned {
                value: 4,
                drops: &drops,
            },
            Owned {
                value: 5,
                drops: &drops,
            },
        ];
        let values: [Owned<'_>; 2] = values.try_into().ok().unwrap();
        assert_eq!(values[0].value + values[1].value, 9);
        assert_eq!(drops.get(), 0);
    }
    assert_eq!(drops.get(), 2);
    {
        let values = vec![Owned {
            value: 6,
            drops: &drops,
        }];
        let capacity = values.capacity();
        let values = <[Owned<'_>; 2]>::try_from(values).err().unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(values[0].value, 6);
    }
    assert_eq!(drops.get(), 3);
    let empty: [u32; 0] = Vec::new().try_into().unwrap();
    assert_eq!(empty.len(), 0);
    let zst: [(); 3] = vec![(); 3].try_into().unwrap();
    assert_eq!(zst.len(), 3);
    let mut queue: VecDeque<u32> = (0..12).collect();
    for n in 0..7 {
        assert_eq!(queue.pop_front(), Some(n));
    }
    queue.extend(12..23);
    queue.push_front(99);
    assert_eq!(
        queue.iter().copied().collect::<Vec<_>>(),
        [
            99, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22
        ]
    );
    let mut iter = queue.into_iter();
    assert_eq!(iter.len(), 17);
    assert_eq!(iter.next(), Some(99));
    assert_eq!(iter.next_back(), Some(22));
    assert_eq!(iter.len(), 15);
    let mut borrowed: VecDeque<&u32> = VecDeque::new();
    let n = 17;
    borrowed.extend([&n]);
    assert_eq!(borrowed.into_iter().next(), Some(&17));
    let mut values = [0u8; 20];
    unsafe {
        let p = values.as_mut_ptr().add(1) as *mut u64;
        p.write_unaligned(0x1020304050607080);
        assert_eq!(p.read_unaligned(), 0x1020304050607080);
        std::ptr::write_unaligned(values.as_mut_ptr().add(11) as *mut u32, 0xf0e0d0c0);
        assert_eq!(
            (values.as_ptr().add(11) as *const u32).read_unaligned(),
            0xf0e0d0c0
        );
    }
    assert_eq!(values[0], 0);
    assert_eq!(values[9], 0);
    assert_eq!(values[10], 0);
    assert_eq!(values[15], 0);
    let mut storage = vec![0u8; std::mem::size_of::<Owned<'_>>() + 1];
    unsafe {
        let destination = storage.as_mut_ptr().add(1) as *mut Owned<'_>;
        destination.write_unaligned(Owned {
            value: 73,
            drops: &drops,
        });
        let mut aligned = std::mem::MaybeUninit::<Owned<'_>>::uninit();
        std::ptr::copy_nonoverlapping(
            destination as *const u8,
            aligned.as_mut_ptr() as *mut u8,
            std::mem::size_of::<Owned<'_>>(),
        );
        let owned = aligned.assume_init();
        assert_eq!(owned.value, 73);
        assert_eq!(drops.get(), 3);
        drop(owned);
    }
    assert_eq!(drops.get(), 4);
    let mut map = HashMap::new();
    map.insert(String::from("key"), 29);
    assert_eq!(map["key"], 29);
    println!("backend APIs {} {} {}", joined, iter.count(), drops.get());
}
