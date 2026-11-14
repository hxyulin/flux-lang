use std::arch::asm;
fn main() {
    let result: u64;
    let mut evaluations = 0;
    let original = 37usize;
    let read = || {
        // SAFETY: only the declared scratch register changes.
        #[cfg(target_arch = "aarch64")]
        unsafe { asm!("add {0}, {0}, #1", inout(reg) original => _, options(nomem, nostack)); }
        #[cfg(target_arch = "x86_64")]
        unsafe { asm!("add {0}, 1", inout(reg) original => _, options(nomem, nostack)); }
        original
    };
    assert_eq!(read(), 37);
    assert_eq!(read(), 37);
    // SAFETY: scratch/output registers are declared; no memory or stack access.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        asm!("add {0}, {1}, #5", "mov {2}, {0}", out(reg) _, in(reg) 37u64, lateout(reg) result, options(pure, nomem, nostack));
        asm!("add {0}, {0}, #1", inout(reg) { evaluations += 1; 7usize } => _, options(nomem, nostack));
        asm!("mov x0, #3", out("x0") _, options(nomem, nostack));
        asm!("movi v0.16b, #1", out("v0") _, options(nomem, nostack));
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("mov {0}, {1}", "add {0}, 5", "mov {2}, {0}", out(reg) _, in(reg) 37u64, lateout(reg) result, options(pure, nomem, nostack));
        asm!("add {0}, 1", inout(reg) { evaluations += 1; 7usize } => _, options(nomem, nostack));
        asm!("mov rax, 3", out("rax") _, options(nomem, nostack));
        asm!("pxor xmm0, xmm0", out("xmm0") _, options(nomem, nostack));
    }
    assert_eq!(result, 42);
    assert_eq!(evaluations, 1);
    assert_eq!(original, 37);
    println!("{} {} {}", result, evaluations, original);
}
