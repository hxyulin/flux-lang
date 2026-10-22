// Compile the actual compiler modules, so regressions in their evolving
// implementations are exercised by both compilers and both flux modes.
// The runtime harness bundles the modules above this driver; flux does
// not yet support the `path` attribute used to import them directly.

use bitset::BitSet;
use rng::Source;

fn main() {
    let mut source = Source::random(core::hint::black_box(108198));
    for _ in 0..16 {
        println!("{} {} {} {} {}", source.below(17), source.range(4, 20),
            source.chance(35), source.weighted(&[0, 1, 3, 0, 7]), source.bits64());
    }
    let tape = source.tape().to_vec();
    let mut a = Source::replay(tape.clone());
    let mut b = Source::replay(tape);
    for _ in 0..120 {
        assert_eq!(a.bits64(), b.bits64());
    }
    println!("exhausted {} {}", a.below(0), a.bits64());

    for size in [0, 1, 63, 64, 65, 127, 128, 129, 257] {
        let mut bits = BitSet::new_empty(size);
        let mut other = BitSet::new_empty(size);
        let full = BitSet::new_filled(size);
        for i in 0..size {
            if i % 3 == 0 { bits.insert(i); }
            other.set(i, i % 5 == 0);
            assert!(full[i]);
        }
        println!("{size}: {} {}", bits.union(&other), bits.union(&other));
        for i in 0..size {
            assert_eq!(bits[i], i % 3 == 0 || i % 5 == 0);
            if i % 2 == 0 { bits.remove(i); }
        }
        let mut cloned = bits.clone();
        assert_eq!(cloned, bits);
        println!("filled {}", cloned.union(&full));
        assert_eq!(cloned, full);
        println!("bits {:?}", bits);
    }
}
