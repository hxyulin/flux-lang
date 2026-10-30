#[cfg(target_arch = "x86_64")]
fn main() {
    let mut word = 17u32;
    let add = 25u32;
    // SAFETY: assembly reads and writes only the declared registers.
    unsafe {
        core::arch::asm!("add {value:e}, {add:e}", value = inout(reg) word, add = in(reg) add, options(pure, nomem, nostack));
    }
    assert_eq!(word, 42);
    let mut small = 0x1234u16;
    // SAFETY: assembly changes only the declared register; it does not use memory.
    unsafe {
        core::arch::asm!("add {0:x}, 3", inout(reg) small, options(nomem, nostack));
    }
    assert_eq!(small, 0x1237);
    let mut wide = 0x1234_5678_0000_0000u64;
    // SAFETY: assembly changes only the declared register.
    unsafe {
        core::arch::asm!("or {0:r}, 17", inout(reg) wide, options(nomem, nostack));
    }
    assert_eq!(wide, 0x1234_5678_0000_0011);
    let original = word;
    // SAFETY: this is the register barrier used by zmij, with no instructions.
    unsafe {
        core::arch::asm!("/*{0:e}*/", inout(reg) word);
    }
    assert_eq!(word, original);
    let value = 99u64;
    // SAFETY: the empty template only names the declared input register.
    unsafe {
        core::arch::asm!("/*{value:r}*/", value = in(reg) value, options(nomem, preserves_flags, nostack));
    }
    let mut double = 1.5f64;
    let sign = 1u64 << 63;
    // SAFETY: register operations change only the declared floating-point bits.
    unsafe {
        core::arch::asm!("xor {value:r}, {sign:r}", value = inout(reg) double, sign = in(reg) sign, options(nomem, nostack));
    }
    assert_eq!(double, -1.5);
    let mut single = 2.5f32;
    let sign = 1u32 << 31;
    // SAFETY: the word register operation changes only the declared float bits.
    unsafe {
        core::arch::asm!("xor {value:e}, {sign:e}", value = inout(reg) single, sign = in(reg) sign, options(nomem, nostack));
    }
    assert_eq!(single, -2.5);
    println!("x86 asm {word} {small} {wide} {double} {single}");
}
#[cfg(not(target_arch = "x86_64"))]
fn main() {}
