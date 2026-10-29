use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Owned(u32);
impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(self.0 as usize, Ordering::Relaxed);
    }
}
fn main() {
    let data = [1, 2, 3];
    let mut seen = 0;
    let mut map = data.iter().map(|x| {
        seen += 1;
        *x + 1
    });
    println!("{:?}", map);
    assert_eq!(map.next(), Some(2));
    println!("{:#?}", map);
    drop(map);
    assert_eq!(seen, 1);
    let mut cloned = data.iter().cloned();
    println!("{:?}", cloned);
    assert_eq!(cloned.next_back(), Some(3));
    println!("{:#?}", cloned);
    let values = vec![vec![1, 2], vec![], vec![3, 4, 5]];
    let mut flat = values.into_iter().flatten();
    println!("{:?}", flat);
    assert_eq!(flat.next(), Some(1));
    assert_eq!(flat.next_back(), Some(5));
    println!("{:#?}", flat);
    assert_eq!(flat.collect::<Vec<_>>(), [2, 3, 4]);
    let references = [vec![7, 8], vec![9]];
    let mut borrowed = references.iter().flatten();
    assert_eq!(borrowed.next(), Some(&7));
    println!("{:?}", borrowed);
    let value = 17;
    let result: Result<&i32, Owned> = Ok(&value);
    assert_eq!(result.copied().ok(), Some(17));
    let mut value = 19;
    let result: Result<&mut i32, Owned> = Ok(&mut value);
    assert_eq!(result.copied().ok(), Some(19));
    let result: Result<&i32, Owned> = Err(Owned(3));
    let mut observed = 0;
    let capture = Owned(5);
    let result = result.copied().inspect_err(move |error| {
        assert_eq!(error.0, 3);
        drop(capture);
    });
    assert_eq!(DROPS.load(Ordering::Relaxed), 5);
    drop(result);
    assert_eq!(DROPS.load(Ordering::Relaxed), 8);
    let ok: Result<u32, Owned> = Ok(2);
    let ok = ok.inspect_err(|_| {
        observed += 1;
    });
    assert_eq!(observed, 0);
    assert_eq!(ok.ok(), Some(2));
    let mut bytes = [3u8, 4, 5];
    let range = bytes.as_mut_ptr_range();
    assert_eq!(range.start, bytes.as_mut_ptr());
    assert_eq!(range.end, unsafe { bytes.as_mut_ptr().add(3) });
    unsafe {
        *range.start = 9;
        *range.end.sub(1) = 7;
    }
    assert_eq!(bytes, [9, 4, 7]);
    let empty: &mut [u8] = &mut [];
    let range = empty.as_mut_ptr_range();
    assert_eq!(range.start, range.end);
    let mut zst = [(); 4];
    let range = zst.as_mut_ptr_range();
    assert_eq!(range.start, range.end);
    let small = AtomicU32::new(u32::MAX);
    let word = AtomicUsize::new(usize::MAX);
    println!("atomics {:?} {:#?}", small, word);
    println!("helpers {} {:?}", DROPS.load(Ordering::Relaxed), bytes);
}
