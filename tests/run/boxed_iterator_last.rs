use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
struct Counter {
    position: u32,
    calls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}
impl Iterator for Counter {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.position == 3 {
            None
        } else {
            self.position += 1;
            Some(self.position)
        }
    }
    fn last(self) -> Option<u32> {
        self.calls.fetch_add(100, Ordering::SeqCst);
        Some(3)
    }
}
impl Drop for Counter {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
fn make(calls: &Arc<AtomicUsize>, drops: &Arc<AtomicUsize>) -> Counter {
    Counter {
        position: 0,
        calls: calls.clone(),
        drops: drops.clone(),
    }
}
fn borrowed<'a>(values: &'a [u32]) -> Option<&'a u32> {
    Box::new(values.iter()).last()
}
fn __sized_dispatch<T>(_: Box<T>) -> u32 {
    900
}
#[repr(C)]
struct Bytes<T: ?Sized> {
    position: usize,
    tail: T,
}
impl Iterator for Bytes<[u8]> {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        if self.position < self.tail.len() {
            let value = self.tail[self.position];
            self.position += 1;
            Some(value)
        } else {
            None
        }
    }
}
struct OwnedValue {
    drops: Arc<AtomicUsize>,
    value: u32,
}
impl Drop for OwnedValue {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
struct Env(Arc<AtomicUsize>);
impl Drop for Env {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct OwnedIterator {
    values: std::vec::IntoIter<OwnedValue>,
    env: Env,
}
impl Iterator for OwnedIterator {
    type Item = OwnedValue;
    fn next(&mut self) -> Option<OwnedValue> {
        self.values.next()
    }
    fn last(self) -> Option<OwnedValue> {
        self.values.last()
    }
}
fn owned(values: &Arc<AtomicUsize>, envs: &Arc<AtomicUsize>) -> OwnedIterator {
    OwnedIterator {
        values: (0..3)
            .map(|value| OwnedValue {
                drops: values.clone(),
                value,
            })
            .collect::<Vec<_>>()
            .into_iter(),
        env: Env(envs.clone()),
    }
}
fn main() {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    assert_eq!(Box::new(make(&calls, &drops)).last(), Some(3));
    assert_eq!(
        (calls.load(Ordering::SeqCst), drops.load(Ordering::SeqCst)),
        (100, 1)
    );
    calls.store(0, Ordering::SeqCst);
    let iter: Box<dyn Iterator<Item = u32>> = Box::new(make(&calls, &drops));
    assert_eq!(iter.last(), Some(3));
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    calls.store(0, Ordering::SeqCst);
    assert_eq!(Box::new(Box::new(make(&calls, &drops))).last(), Some(3));
    assert_eq!(calls.load(Ordering::SeqCst), 100);
    calls.store(0, Ordering::SeqCst);
    let iter: Box<dyn Iterator<Item = u32>> = Box::new(make(&calls, &drops));
    assert_eq!(Box::new(iter).last(), Some(3));
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    calls.store(0, Ordering::SeqCst);
    let f: fn(Box<Counter>) -> Option<u32> = <Box<Counter> as Iterator>::last;
    assert_eq!(f(Box::new(make(&calls, &drops))), Some(3));
    assert_eq!(calls.load(Ordering::SeqCst), 100);
    assert_eq!(drops.load(Ordering::SeqCst), 5);
    let bytes: Box<Bytes<[u8]>> = Box::new(Bytes {
        position: 0,
        tail: [4u8, 7, 9],
    });
    assert_eq!(bytes.last(), Some(9));
    let values = [7u32, 11, 19];
    assert_eq!(*borrowed(&values).unwrap(), 19);
    let borrowed_dyn: Box<dyn Iterator<Item = &u32> + '_> = Box::new(values.iter());
    let last = borrowed_dyn.last().unwrap();
    assert_eq!(*last, 19);
    assert_eq!(last as *const u32, &values[2] as *const u32);
    assert_eq!(__sized_dispatch(Box::new(1)), 900);
    let values = Arc::new(AtomicUsize::new(0));
    let envs = Arc::new(AtomicUsize::new(0));
    let result = Box::new(owned(&values, &envs)).last().unwrap();
    assert_eq!(result.value, 2);
    assert_eq!(
        (values.load(Ordering::SeqCst), envs.load(Ordering::SeqCst)),
        (2, 1)
    );
    drop(result);
    let iter: Box<dyn Iterator<Item = OwnedValue>> = Box::new(owned(&values, &envs));
    let result = iter.last().unwrap();
    assert_eq!(result.value, 2);
    assert_eq!(
        (values.load(Ordering::SeqCst), envs.load(Ordering::SeqCst)),
        (5, 2)
    );
    drop(result);
    assert_eq!(values.load(Ordering::SeqCst), 6);
    println!(
        "boxed last {} {}",
        calls.load(Ordering::SeqCst),
        drops.load(Ordering::SeqCst)
    );
}
