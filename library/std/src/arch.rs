//! Architecture intrinsics and bounded runtime feature detection.

#[cfg(target_arch = "aarch64")]
pub use core::arch::*;
pub use crate::__is_aarch64_feature_detected as is_aarch64_feature_detected;

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
