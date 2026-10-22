// Misused iterator adapters: `copied` of items that are not references,
// `cycle` of an iterator that is not `Clone`, `rposition` of one that
// does not know its length, and sums of references to other types.

struct Counter {
    n: u32,
}

impl Iterator for Counter {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        self.n += 1;
        Some(self.n)
    }
}

fn main() {
    let _ = (0..3).copied();
    let _ = Counter { n: 0 }.cycle();
    let _ = (0..3).filter(|x| *x > 1).rposition(|x| x == 2);
    let s: &[u8] = &[1, 2];
    let _: u32 = s.iter().sum();
}
