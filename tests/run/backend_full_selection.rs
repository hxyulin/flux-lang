use std::hint::black_box;

fn offset_load(values: &[u64; 4]) -> u64 {
    values[1].wrapping_add(values[3])
}

fn shifted_add(base: u64, index: u64, tail: u64) -> u64 {
    base.wrapping_add(index << 3).wrapping_add(tail)
}

fn low_bits_clear(value: u64) -> bool {
    value & 15 == 0
}

fn main() {
    let values = black_box([7, 11, 23, 29]);
    let loaded = offset_load(&values);
    let computed = shifted_add(black_box(7), black_box(11), black_box(23));
    assert_eq!(loaded, 40);
    assert_eq!(computed, 118);
    assert!(low_bits_clear(black_box(16)));
    assert!(!low_bits_clear(black_box(17)));
    println!("full selection {loaded} {computed}");
}
