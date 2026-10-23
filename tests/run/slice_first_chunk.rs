#[repr(align(64))]
struct Aligned(u64);

fn check<const N: usize>(s: &[u64]) {
    match s.split_first_chunk::<N>() {
        None => assert!(s.len() < N),
        Some((prefix, suffix)) => {
            assert!(s.len() >= N);
            assert_eq!(prefix.as_ptr(), s.as_ptr());
            assert_eq!(suffix.as_ptr(), unsafe { s.as_ptr().add(N) });
            assert_eq!(suffix.len(), s.len() - N);
            for i in 0..N {
                assert_eq!(prefix[i], s[i]);
            }
            for i in 0..suffix.len() {
                assert_eq!(suffix[i], s[N + i]);
            }
        }
    }
}

fn main() {
    let data = [10, 20, 30, 40, 50, 60];
    for start in 0..7 {
        for end in start..7 {
            let s = &data[start..end];
            check::<0>(s);
            check::<1>(s);
            check::<4>(s);
            check::<6>(s);
            check::<7>(s);
        }
    }
    let aligned = [Aligned(11), Aligned(22), Aligned(33)];
    let s = &aligned[1..];
    let (prefix, suffix) = s.split_first_chunk::<1>().unwrap();
    assert_eq!(prefix.as_ptr(), s.as_ptr());
    assert_eq!(prefix[0].0, 22);
    assert_eq!(suffix[0].0, 33);
    assert_eq!(prefix.as_ptr() as usize % 64, 0);
    let zst: &[()] = &[(), (), ()];
    let (prefix, suffix) = zst.split_first_chunk::<2>().unwrap();
    assert_eq!(prefix.len(), 2);
    assert_eq!(suffix.len(), 1);
    let (empty, whole) = zst.split_first_chunk::<0>().unwrap();
    assert_eq!(empty.len(), 0);
    assert_eq!(whole.len(), 3);
    println!("slice-first-chunk-ok {} {}", prefix.len(), suffix.len());
}
