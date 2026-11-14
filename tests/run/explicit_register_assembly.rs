use std::arch::asm;
fn main() {
    let mut value = 37usize;
    let result: usize;
    let float_result: f64;
    let narrow: u32;
    // SAFETY: only the declared integer registers change; no memory is read.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("add x0, x0, #5", inout("x0") value, options(nomem, nostack));
        asm!("add x1, x0, #7", in("x0") value, lateout("x1") result, options(nomem, nostack));
        asm!("fadd d0, d0, d0", inlateout("d0") 2.5f64 => float_result, options(nomem, nostack));
        asm!("add w2, w2, #4", inlateout("w2") 3u32 => narrow, options(nomem, nostack));
        asm!("mov fp, fp", in("x0") value, options(nomem, nostack, preserves_flags));
    }
    // SAFETY: only the declared integer registers change; no memory is read.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("add rax, 5", inout("rax") value, options(nomem, nostack));
        asm!("lea rcx, [rax + 7]", in("rax") value, lateout("rcx") result, options(nomem, nostack));
        asm!("addsd xmm0, xmm0", inlateout("xmm0") 2.5f64 => float_result, options(nomem, nostack));
        asm!("add edx, 4", inlateout("edx") 3u32 => narrow, options(nomem, nostack));
    }
    println!("{} {} {:.1} {}", value, result, float_result, narrow);
}
