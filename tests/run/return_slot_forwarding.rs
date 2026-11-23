type Wide = (u64, u64, u64, u64, u64, u64, u64, u64);

#[inline(never)]
fn produce(seed: u64) -> Wide {
    println!("produce {seed}");
    (
        seed,
        seed + 1,
        seed + 2,
        seed + 3,
        seed + 4,
        seed + 5,
        seed + 6,
        seed + 7,
    )
}

#[inline(never)]
fn forward(seed: u64) -> Wide {
    if seed % 2 == 0 {
        return produce(seed);
    }
    produce(seed + 2)
}

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        println!("drop");
    }
}

#[inline(never)]
fn cleanup(seed: u64) -> Wide {
    let _guard = Guard;
    produce(seed + 4)
}

fn main() {
    for seed in [13, 14] {
        let (a, b, c, d, e, f, g, h) = std::hint::black_box(forward(seed));
        println!("{a} {b} {c} {d} {e} {f} {g} {h}");
    }
    let value = cleanup(std::hint::black_box(13));
    println!("{} {}", value.0, value.7);
}
