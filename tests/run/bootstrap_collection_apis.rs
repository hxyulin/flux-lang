use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;

struct Item(u32, Rc<Cell<u32>>);
impl Drop for Item {
    fn drop(&mut self) {
        self.1.set(self.1.get() + 1);
    }
}

static ZST_DROPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
struct Zero;
impl Drop for Zero {
    fn drop(&mut self) {
        ZST_DROPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}
#[repr(align(64))]
struct Aligned(u32);

fn chunk<const N: usize>(s: &[u32]) {
    match s.split_last_chunk::<N>() {
        None => assert!(s.len() < N),
        Some((prefix, last)) => {
            assert_eq!(prefix.len(), s.len() - N);
            assert_eq!(prefix.as_ptr(), s.as_ptr());
            assert_eq!(last.as_ptr(), unsafe { s.as_ptr().add(prefix.len()) });
            for i in 0..N {
                assert_eq!(last[i], s[prefix.len() + i]);
            }
        }
    }
}

fn main() {
    let mut checksum = 0;
    for head in 0..8 {
        for start in 0..=8 {
            for end in start..=8 {
                let mut q = VecDeque::with_capacity(8);
                q.extend(0..8);
                for i in 0..head {
                    assert_eq!(q.pop_front(), Some(i));
                    q.push_back(i + 8);
                }
                let before: Vec<_> = q.iter().copied().collect();
                let mut drain = q.drain(start..end);
                assert_eq!(drain.len(), end - start);
                let mut front = start;
                let mut back = end;
                if front < back {
                    assert_eq!(drain.next(), Some(before[front]));
                    front += 1;
                }
                if front < back {
                    back -= 1;
                    assert_eq!(drain.next_back(), Some(before[back]));
                }
                assert_eq!(drain.size_hint(), (back - front, Some(back - front)));
                // Dropping a partially consumed drain must also remove its middle.
                drop(drain);
                let expected: Vec<_> = before
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i < start || *i >= end)
                    .map(|(_, v)| *v)
                    .collect();
                assert_eq!(q.iter().copied().collect::<Vec<_>>(), expected);
                q.push_front(99);
                q.push_back(100);
                checksum += q.into_iter().sum::<u32>();
            }
        }
    }
    let mut q = VecDeque::from([1, 2, 3, 4]);
    assert_eq!(q.drain(1..=2).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(q.drain(..1).next_back(), Some(1));
    assert_eq!(q.drain(0..).next(), Some(4));
    let mut empty = q.drain(..);
    assert_eq!(empty.next(), None);
    assert_eq!(empty.next_back(), None);
    assert_eq!(empty.len(), 0);
    drop(empty);

    let drops = Rc::new(Cell::new(0));
    {
        let mut q = VecDeque::with_capacity(8);
        for i in 0..8 {
            q.push_back(Item(i, drops.clone()));
        }
        for _ in 0..3 {
            drop(q.pop_front());
        }
        for i in 8..11 {
            q.push_back(Item(i, drops.clone()));
        }
        let mut d = q.drain(1..6);
        assert_eq!(d.next().unwrap().0, 4);
        assert_eq!(d.next_back().unwrap().0, 8);
        drop(d);
        assert_eq!(drops.get(), 8);
        assert_eq!(q.iter().map(|v| v.0).collect::<Vec<_>>(), vec![3, 9, 10]);
    }
    assert_eq!(drops.get(), 11);
    let mut zst = VecDeque::from([(); 8]);
    assert_eq!(zst.drain(2..6).count(), 4);
    assert_eq!(zst.len(), 4);
    assert_eq!(zst.drain(..).count(), 4);
    {
        let mut q = VecDeque::new();
        for _ in 0..8 {
            q.push_back(Zero);
        }
        let mut d = q.drain(2..6);
        drop(d.next());
        drop(d.next_back());
        drop(d);
        assert_eq!(ZST_DROPS.load(std::sync::atomic::Ordering::Relaxed), 4);
        assert_eq!(q.len(), 4);
    }
    assert_eq!(ZST_DROPS.load(std::sync::atomic::Ordering::Relaxed), 8);
    let mut aligned = VecDeque::from([Aligned(11), Aligned(22), Aligned(33)]);
    assert_eq!(
        (aligned.front().unwrap() as *const Aligned as usize) % 64,
        0
    );
    assert_eq!(aligned.drain(1..2).next().unwrap().0, 22);
    assert_eq!(aligned.pop_back().unwrap().0, 33);
    let data = [10, 20, 30, 40];
    for start in 0..=4 {
        for end in start..=4 {
            let s = &data[start..end];
            chunk::<0>(s);
            chunk::<1>(s);
            chunk::<2>(s);
            chunk::<4>(s);
            chunk::<5>(s);
        }
    }
    let zst: &[()] = &[(); 3];
    assert_eq!(zst.split_last_chunk::<0>().unwrap().0.len(), 3);
    assert_eq!(zst.split_last_chunk::<2>().unwrap().1.len(), 2);
    println!("bootstrap collection APIs {checksum} {}", drops.get());
}
