#[inline(never)]
fn mixed_product(a: u128, b: u64) -> u128 {
    a.wrapping_mul(b as u128)
}

#[inline(never)]
fn folded_product(a: u64, b: u64) -> u64 {
    let product = (a as u128) * (b as u128);
    product as u64 ^ (product >> 64) as u64
}

#[inline(never)]
fn wide_shift(a: u128) -> (u128, u128, i128) {
    (a << 64, a >> 64, (a as i128) >> 64)
}

fn main() {
    let values = [0, 1, 0x1234_5678_9abc_def0, 1 << 63, u64::MAX];
    for a in values {
        for b in values {
            let a = std::hint::black_box(a);
            let b = std::hint::black_box(b);
            let wide = ((a as u128) << 64) | u128::from(b);
            let (l, r, s) = wide_shift(wide);
            println!("{:032x} {:016x} {:032x} {:032x} {:032x}",
                mixed_product(wide, a), folded_product(a, b), l, r, s);
        }
    }
}
