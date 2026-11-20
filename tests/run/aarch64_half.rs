use core::arch::{
    aarch64::{float64x2_t, uint16x4_t},
    asm,
};
use core::mem::transmute;

#[target_feature(enable = "fp16")]
unsafe fn round_trip(values: [u16; 4]) -> ([f64; 2], [u16; 4]) {
    // SAFETY: each pair of representations covers the same initialized lanes.
    let input: uint16x4_t = unsafe { transmute(values) };
    let wide: float64x2_t;
    let narrow: uint16x4_t;
    // SAFETY: the function enables FP16 and the assembly only uses its operands.
    unsafe {
        asm!(
            "fcvtl {temp:v}.4s, {input:v}.4h",
            "fcvtl {wide:v}.2d, {temp:v}.2s",
            temp = out(vreg) _,
            input = in(vreg) input,
            wide = lateout(vreg) wide,
            options(pure, nomem, nostack),
        );
        asm!(
            "fcvtn {temp:v}.2s, {wide:v}.2d",
            "fcvtn2 {temp:v}.4s, {wide:v}.2d",
            "fcvtn {narrow:v}.4h, {temp:v}.4s",
            temp = out(vreg) _,
            wide = in(vreg) wide,
            narrow = lateout(vreg) narrow,
            options(pure, nomem, nostack),
        );
        (transmute(wide), transmute(narrow))
    }
}

fn main() {
    if std::arch::is_aarch64_feature_detected!("fp16") {
        // SAFETY: the runtime query checked that FP16 is available.
        let (wide, narrow) = unsafe { round_trip([0x3c00, 0xc000, 0, 0]) };
        assert_eq!(wide, [1.0, -2.0]);
        assert_eq!(narrow, [0x3c00, 0xc000, 0x3c00, 0xc000]);
        println!("{wide:?} {narrow:?}");
    }
}
