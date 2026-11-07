#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "dit")]
unsafe fn round_trip() -> u64 {
    let previous: u64;
    // SAFETY: this function requires DIT; these instructions access only DIT and registers.
    unsafe {
        std::arch::asm!("mrs {0}, dit", lateout(reg) previous, options(nomem, nostack));
        std::arch::asm!("msr dit, #{0}", const 1, options(nostack));
    }
    let enabled: u64;
    // SAFETY: DIT is available and the original state is restored before returning.
    unsafe {
        std::arch::asm!("mrs {0}, dit", lateout(reg) enabled, options(nomem, nostack));
        std::arch::asm!("msr dit, {0}", in(reg) previous, options(nostack));
    }
    enabled >> 24
}
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "sb")]
unsafe fn barrier() {
    // SAFETY: the caller checks SB support; the instruction is a speculation barrier.
    unsafe { std::arch::asm!("sb", options(nostack)); }
}
#[cfg(target_arch = "aarch64")]
fn main() {
    if std::arch::is_aarch64_feature_detected!("dit") {
        // SAFETY: runtime detection established DIT support.
        println!("dit {}", unsafe { round_trip() });
    } else { println!("no dit"); }
    let sb = std::arch::is_aarch64_feature_detected!("sb");
    if sb {
        // SAFETY: runtime detection established SB support.
        unsafe { barrier(); }
    }
    println!("sb {}", sb);
}
#[cfg(not(target_arch = "aarch64"))]
fn main() {}
