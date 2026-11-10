use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, VecDeque};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Counted(usize, Arc<AtomicUsize>);
impl Drop for Counted {
    fn drop(&mut self) { self.1.fetch_add(1, Ordering::SeqCst); }
}
fn main() {
    for capacity in 1..16 {
        for head in 0..capacity {
            for len in 0..=capacity {
                let drops = Arc::new(AtomicUsize::new(0));
                let mut deque = VecDeque::with_capacity(capacity);
                // Advance the ring without creating live Counted values.
                for _ in 0..head { deque.push_back(Counted(999, drops.clone())); drop(deque.pop_front()); }
                let before = drops.load(Ordering::SeqCst);
                for i in 0..len { deque.push_back(Counted(i, drops.clone())); }
                let cap = deque.capacity();
                let vector: Vec<_> = deque.into();
                assert_eq!(vector.capacity(), cap);
                assert_eq!(vector.iter().map(|x| x.0).collect::<Vec<_>>(), (0..len).collect::<Vec<_>>());
                assert_eq!(drops.load(Ordering::SeqCst), before);
                drop(vector);
                assert_eq!(drops.load(Ordering::SeqCst), before + len);
            }
        }
    }
    let mut vector = Vec::with_capacity(17);
    vector.extend([2, 4, 6]);
    let pointer = vector.as_ptr();
    let deque: VecDeque<_> = vector.into();
    assert_eq!(deque.as_slices().0.as_ptr(), pointer);
    let vector: Vec<_> = deque.into();
    assert_eq!(vector.as_ptr(), pointer);
    let boxed = vector.into_boxed_slice();
    let pointer = boxed.as_ptr();
    let vector: Vec<_> = boxed.into();
    assert_eq!(vector.as_ptr(), pointer);
    let mut heap = BinaryHeap::from(vec![3, 1, 4]);
    let mut drained: Vec<_> = heap.drain().collect();
    drained.sort();
    assert_eq!(drained, [1, 3, 4]);
    assert!(heap.is_empty());
    let vector: Vec<_> = BinaryHeap::from(vec![7, 2]).into();
    assert_eq!(vector.len(), 2);
    let zsts: VecDeque<()> = vec![(); 9].into();
    let zsts: Vec<()> = zsts.into();
    assert_eq!(zsts.len(), 9);

    let mut data = [0, 1, 2, 3, 4, 5, 6];
    let mut chunks = data.rchunks_mut(3);
    assert_eq!(chunks.len(), 3);
    chunks.next().unwrap()[0] = 40;
    chunks.next_back().unwrap()[0] = 10;
    chunks.next().unwrap()[0] = 20;
    assert!(chunks.next().is_none());
    assert_eq!(data, [10, 20, 2, 3, 40, 5, 6]);
    let mut exact = data.rchunks_exact_mut(3);
    exact.next_back().unwrap()[0] = 21;
    exact.next().unwrap()[0] = 41;
    exact.into_remainder()[0] = 11;
    assert_eq!(data, [11, 21, 2, 3, 41, 5, 6]);
    let mut split_data = [1, 0, 2, 0, 3];
    let mut split = split_data.split_mut(|x| *x == 0);
    split.next_back().unwrap()[0] = 30;
    split.next().unwrap()[0] = 10;
    split.next_back().unwrap()[0] = 20;
    assert!(split.next().is_none());
    assert_eq!(split_data, [10, 0, 20, 0, 30]);
    let mut inclusive = [1, 0, 2, 0, 3];
    let mut split = inclusive.split_inclusive_mut(|x| *x == 0);
    assert_eq!(split.next_back().unwrap(), [3]);
    assert_eq!(split.next().unwrap(), [1, 0]);
    assert_eq!(split.next_back().unwrap(), [2, 0]);
    assert!(split.next().is_none());
    let mut groups = [1, 1, 2, 3, 3];
    let mut iter = groups.chunk_by_mut(|a, b| a == b);
    assert_eq!(iter.next_back().unwrap(), [3, 3]);
    assert_eq!(iter.next().unwrap(), [1, 1]);
    assert_eq!(iter.next_back().unwrap(), [2]);
    assert!(iter.next().is_none());
    for len in 0..15 {
        for step in 1..8 {
            let expected: Vec<_> = (0..len).step_by(step).collect();
            let mut iter = (0..len).step_by(step);
            let mut front = 0;
            let mut back = expected.len();
            while front < back {
                assert_eq!(iter.next_back(), Some(expected[back - 1])); back -= 1;
                if front < back { assert_eq!(iter.next(), Some(expected[front])); front += 1; }
            }
            assert!(iter.next_back().is_none());
            assert!(iter.next().is_none());
        }
    }
    let values = [2, 3, 2];
    let mut set: BTreeSet<i32> = BTreeSet::new(); set.extend(values.iter()); assert_eq!(set.len(), 2);
    let pairs = [(1, 10), (2, 20)];
    let mut tree: BTreeMap<i32, i32> = BTreeMap::new(); tree.extend(pairs.iter().map(|(k, v)| (k, v)));
    let mut hash: HashMap<i32, i32> = HashMap::new(); hash.extend(pairs.iter().map(|(k, v)| (k, v)));
    assert_eq!(tree.get(&2), Some(&20)); assert_eq!(hash.get(&1), Some(&10));
    let mut os = std::ffi::OsString::new(); os.extend([std::ffi::OsStr::new("a"), std::ffi::OsStr::new("b")]); assert_eq!(os, "ab");
    println!("collection conversions, drains, and mutable iterators passed");
}
