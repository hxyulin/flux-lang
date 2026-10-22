//! Safe calls never enter the unreachable intrinsic branch.
fn extract(value: Option<u32>) -> u32 {
    match value {
        Some(n) => n,
        // SAFETY: callers only pass Some values.
        None => unsafe { core::hint::unreachable_unchecked() },
    }
}
fn main() {
    for n in [0, 1, 31, 255, 0xffff_ffff] {
        println!("{}", extract(core::hint::black_box(Some(n))));
    }
}
