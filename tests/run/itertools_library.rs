use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(i32);
impl Drop for Owned {
    fn drop(&mut self) { DROPS.fetch_add(1, Ordering::Relaxed); }
}
fn main() {
    const FIRST: Option<&u32> = [7u32, 11].as_slice().first();
    const LAST: Option<&u32> = [7u32, 11].as_slice().last();
    const EMPTY: Option<&u32> = [].as_slice().first();
    println!("{:?} {:?} {:?}", FIRST, LAST, EMPTY);
    let mut array = [1, 2, 3].into_iter();
    let _ = array.next();
    let _ = array.next_back();
    println!("{:?}", array);
    let mut deque: VecDeque<_> = (0..7).collect();
    for _ in 0..4 { let value = deque.pop_front().unwrap(); deque.push_back(value); }
    deque.rotate_left(2);
    assert_eq!(deque.iter().copied().collect::<Vec<_>>(), [6,0,1,2,3,4,5]);
    deque.rotate_right(2);
    assert_eq!(deque.iter().copied().collect::<Vec<_>>(), [4,5,6,0,1,2,3]);
    deque.rotate_left(7);
    deque.rotate_right(0);
    let mut empty: VecDeque<i32> = VecDeque::new(); empty.rotate_left(0); empty.rotate_right(0);
    let mut owned: VecDeque<_> = (0..5).map(Owned).collect();
    owned.rotate_right(2); owned.rotate_left(4);
    assert_eq!(owned.iter().map(|value| value.0).collect::<Vec<_>>(), [2,3,4,0,1]);
    drop(owned); assert_eq!(DROPS.load(Ordering::Relaxed), 5);
    let original: Vec<_> = (0..113).map(|i| (i * 37) % 19).collect();
    let mut sorted = original.clone(); sorted.sort();
    for index in 0..original.len() {
        let mut values = original.clone();
        let (before, pivot, after) = values.select_nth_unstable(index);
        assert_eq!(*pivot, sorted[index]);
        assert!(before.iter().all(|value| value <= pivot));
        assert!(after.iter().all(|value| value >= pivot));
    }
    let mut values = [3i32, 8, 1, 4, 9];
    let (_, pivot, _) = values.select_nth_unstable_by(2, |a: &i32, b: &i32| b.cmp(a)); assert_eq!(*pivot, 4);
    let mut values = [3i32, -8, 1, -4, 9];
    let (_, pivot, _) = values.select_nth_unstable_by_key(2, |value: &i32| value.abs()); assert_eq!(*pivot, -4);
    let ok: Result<_, ()> = Ok(Owned(17));
    let mut iter = ok.iter(); let mut cloned = iter.clone();
    assert_eq!(iter.len(), 1); assert_eq!(iter.next().unwrap().0, 17);
    assert!(iter.next_back().is_none()); assert_eq!(cloned.next_back().unwrap().0, 17);
    let mut result: Result<_, i32> = Ok(23);
    *result.iter_mut().next().unwrap() += 1;
    assert_eq!(result.into_iter().collect::<Vec<_>>(), [24]);
    let err: Result<i32, _> = Err(Owned(19));
    assert_eq!(err.iter().len(), 0); assert!(err.into_iter().next().is_none());
    let mut map = HashMap::new(); map.insert(1, Owned(29));
    let mut keys = map.keys().clone(); assert_eq!(keys.next(), Some(&1));
    let mut values = map.values().clone(); assert_eq!(values.next().unwrap().0, 29);
    let set: HashSet<_> = [31].into_iter().collect(); let mut items = set.iter().clone();
    assert_eq!(items.next(), Some(&31));
    let values = [1, 2, 3, 4];
    for mid in 0..=values.len() {
        let (left, right) = unsafe { values.split_at_unchecked(mid) };
        assert_eq!(left, &values[..mid]); assert_eq!(right, &values[mid..]);
    }
    let mut values = [1, 2, 3, 4];
    let (left, right) = unsafe { values.split_at_mut_unchecked(2) };
    left[1] = 7; right[0] = 9; assert_eq!(values, [1, 7, 9, 4]);
    println!("itertools library APIs passed");
}
