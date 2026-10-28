trait Size {
    const N: usize;
}
struct Three;
impl Size for Three {
    const N: usize = 3;
}
struct Layout<T>(T);
impl<T> Layout<T> {
    const BYTES: usize = core::mem::size_of::<T>();
}
struct Count<const N: usize>;
impl<const N: usize> Count<N> {
    const NEXT: usize = N + 1;
}
struct Packet;
impl Packet {
    const LEN: usize = 5;
    fn bytes() -> [u8; Self::LEN] {
        [9; Self::LEN]
    }
}
const fn length(n: usize) -> usize {
    let mut i = 0;
    let mut sum = 0;
    while i < n {
        sum += i;
        i += 1;
    }
    sum
}
const BASE: usize = 1 + 2;
const NESTED: usize = [0u8; BASE].len() + 1;
const VALUES: [usize; 3] = [1, 4, 7];
type TraitArray = [u8; <Three as Size>::N];
fn main() {
    let a: TraitArray = [2; Three::N];
    let b: [u8; Layout::<u32>::BYTES] = [3; 4];
    let c: [u8; Layout::<u64>::BYTES] = [4; 8];
    let d: [u8; Count::<3>::NEXT] = [5; 4];
    let e: [u8; length(4)] = [6; 6];
    let f: [u8; {
        let n = 2;
        n * n + 1
    }] = [7; 5];
    let g: [u8; VALUES[1] + 1] = [8; 5];
    let h: [u8; NESTED] = [9; 4];
    assert_eq!(a, [2, 2, 2]);
    assert_eq!(b, [3; 4]);
    assert_eq!(c, [4; 8]);
    assert_eq!(d, [5; 4]);
    assert_eq!(e, [6; 6]);
    assert_eq!(f, [7; 5]);
    assert_eq!(g, [8; 5]);
    assert_eq!(h, [9; 4]);
    assert_eq!(Packet::bytes(), [9; 5]);
    println!(
        "array constants {} {} {} {} {} {} {} {}",
        a.len(),
        b.len(),
        c.len(),
        d.len(),
        e.len(),
        f.len(),
        g.len(),
        h.len()
    );
}
