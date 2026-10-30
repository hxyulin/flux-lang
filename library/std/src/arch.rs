//! Architecture intrinsics and bounded runtime feature detection.

pub use crate::__is_aarch64_feature_detected as is_aarch64_feature_detected;
pub use core::arch::*;

#[doc(hidden)]
pub fn __crc_detected() -> bool {
    if cfg!(target_feature = "crc") {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        query(b"hw.optional.armv8_crc32\0") || query(b"hw.optional.arm.FEAT_CRC32\0")
    }
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn getauxval(kind: usize) -> usize;
        }
        // SAFETY: AT_HWCAP is a scalar query with no pointer arguments.
        unsafe { getauxval(16) & (1 << 7) != 0 }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn query(name: &[u8]) -> bool {
    unsafe extern "C" {
        fn sysctlbyname(
            name: *const u8,
            old: *mut u8,
            old_len: *mut usize,
            new: *mut u8,
            new_len: usize,
        ) -> i32;
    }
    let mut value = 0i32;
    let mut length = core::mem::size_of::<i32>();
    // SAFETY: names are NUL-terminated literals, value has the advertised
    // writable size, and the null new pointer requests a read-only query.
    let result = unsafe {
        sysctlbyname(
            name.as_ptr(),
            &mut value as *mut i32 as *mut u8,
            &mut length,
            core::ptr::null_mut(),
            0,
        )
    };
    result == 0 && length == core::mem::size_of::<i32>() && value != 0
}

#[cfg(target_arch = "x86_64")]
pub use crate::is_x86_feature_detected;

/// Runtime feature query used by the x86 detection macro.
#[doc(hidden)]
#[cfg(target_arch = "x86_64")]
pub fn __x86_detected(feature: &str) -> bool {
    use core::arch::x86_64::{__cpuid_count, _xgetbv};
    // SAFETY: CPUID is available on all x86-64 processors.
    let leaf = unsafe { __cpuid_count(1, 0) };
    match feature {
        "sse" => leaf.edx & (1 << 25) != 0,
        "sse2" => leaf.edx & (1 << 26) != 0,
        "sse3" => leaf.ecx & 1 != 0,
        "ssse3" => leaf.ecx & (1 << 9) != 0,
        "sse4.1" => leaf.ecx & (1 << 19) != 0,
        "sse4.2" => leaf.ecx & (1 << 20) != 0,
        "avx" | "avx2" => {
            // AVX requires XSAVE, OSXSAVE, hardware AVX and OS-managed XMM/YMM state.
            if leaf.ecx & (7 << 26) != 7 << 26 {
                return false;
            }
            // SAFETY: OSXSAVE was checked above; only XCR0 is queried.
            if unsafe { _xgetbv(0) } & 6 != 6 {
                return false;
            }
            if feature == "avx" {
                return true;
            }
            // SAFETY: CPUID is available on all x86-64 processors.
            unsafe { __cpuid_count(0, 0).eax >= 7 && __cpuid_count(7, 0).ebx & (1 << 5) != 0 }
        }
        _ => false,
    }
}
