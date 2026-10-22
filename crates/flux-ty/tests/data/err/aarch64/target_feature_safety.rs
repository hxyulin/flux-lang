use core::arch::aarch64::*;

#[target_feature(enable = "neon")]
unsafe fn annotated(x: u8) -> u8 { x }

fn apply<F: Fn(u8) -> u8>(f: F) -> u8 { f(1) }

fn main() {
    annotated(1);
    let f: unsafe fn(u8) -> u8 = annotated;
    f(2);
    let safe: fn(u8) -> u8 = annotated;
    apply(annotated);
    vdupq_n_u8(1);
    let splat: fn(u8) -> uint8x16_t = vdupq_n_u8;
}

trait Safe { fn run(&self); }
struct S;
impl Safe for S {
    #[target_feature(enable = "neon")]
    unsafe fn run(&self) {}
}

trait Unsafe { unsafe fn run(&self); }
impl Unsafe for S {
    fn run(&self) {}
}
