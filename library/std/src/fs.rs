//! Byte-preserving Unix source-file reads and canonical paths.
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;
use crate::ffi::OsString;
use crate::io::{Error, Result};
use crate::path::{Path, PathBuf};

mod native {
    unsafe extern "C" {
        pub fn open(path: *const core::ffi::c_char, flags: i32, ...) -> i32;
        pub fn read(fd: i32, buffer: *mut core::ffi::c_void, count: usize) -> isize;
        pub fn close(fd: i32) -> i32;
        pub fn stat(path: *const u8, buffer: *mut u8) -> i32;
        pub fn realpath(path: *const core::ffi::c_char, resolved: *mut core::ffi::c_char) -> *mut core::ffi::c_char;
        pub fn free(pointer: *mut core::ffi::c_void);
    }
}
#[cfg(target_os = "macos")]
const READ_FLAGS: i32 = 0x1000000; // O_RDONLY | O_CLOEXEC.
#[cfg(target_os = "linux")]
const READ_FLAGS: i32 = 0x80000; // O_RDONLY | O_CLOEXEC.

fn c_path(path: &Path) -> Result<Vec<u8>> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.contains(&0) { return Err(Error::nul_path()); }
    let mut result = bytes.to_vec(); result.push(0); Ok(result)
}
struct Descriptor(i32);
impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: this owner holds the descriptor returned by open and closes
        // it once. Retrying close could affect a subsequently reused number.
        unsafe { native::close(self.0); }
    }
}

pub fn read<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let name = c_path(path.as_ref())?;
    // SAFETY: name is NUL-terminated; flags select read-only, close-on-exec.
    let raw = unsafe { native::open(name.as_ptr() as *const core::ffi::c_char, READ_FLAGS) };
    if raw < 0 { return Err(Error::last_os_error()); }
    let fd = Descriptor(raw);
    let mut result = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        // SAFETY: fd remains live, and buffer has precisely the supplied writable
        // byte capacity. Only bytes actually returned by read are appended.
        let count = unsafe { native::read(fd.0, buffer.as_mut_ptr() as *mut core::ffi::c_void, buffer.len()) };
        if count == 0 { return Ok(result); }
        if count < 0 {
            let error = Error::last_os_error();
            if error.raw_os_error() == Some(4) { continue; }
            return Err(error);
        }
        result.extend_from_slice(&buffer[..count as usize]);
    }
}
pub fn read_to_string<P: AsRef<Path>>(path: P) -> Result<String> {
    String::from_utf8(read(path)?).map_err(|_| Error::invalid_utf8())
}

pub(crate) fn is_file(path: &Path) -> bool {
    let name = match c_path(path) { Ok(name) => name, Err(_) => return false };
    // Darwin and the supported 64-bit Linux stat records fit in 256 bytes
    // with alignment at most eight. The only interpreted field is st_mode;
    // all other bytes, including padding, remain opaque initialized storage.
    let mut storage = [0u64; 32];
    // SAFETY: name is NUL-terminated and storage is aligned, initialized and
    // larger than the native stat record on each admitted target.
    if unsafe { native::stat(name.as_ptr(), storage.as_mut_ptr() as *mut u8) } != 0 { return false; }
    #[cfg(target_os = "macos")]
    // SAFETY: Darwin st_mode is a u16 at offset four in struct stat.
    let mode = unsafe { *((storage.as_ptr() as *const u8).add(4) as *const u16) } as u32;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    // SAFETY: Linux x86-64 st_mode is a u32 at offset 24.
    let mode = unsafe { *((storage.as_ptr() as *const u8).add(24) as *const u32) };
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    // SAFETY: Linux AArch64 st_mode is a u32 at offset 16.
    let mode = unsafe { *((storage.as_ptr() as *const u8).add(16) as *const u32) };
    mode & 0xf000 == 0x8000
}

/// Copies a non-null live C string. The caller guarantees termination and
/// readability through the first NUL; no reference escapes its allocation.
pub(crate) unsafe fn copy_c_string(raw: *const u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut index = 0;
    // SAFETY: the caller guarantees the source through its first NUL.
    unsafe { while *raw.add(index) != 0 { bytes.push(*raw.add(index)); index += 1; } }
    bytes
}
struct Resolved(*mut core::ffi::c_char);
impl Drop for Resolved {
    fn drop(&mut self) {
        // SAFETY: realpath(NULL output) returned this malloc allocation, and
        // the guard owns it until after its bytes have been copied.
        unsafe { native::free(self.0 as *mut core::ffi::c_void); }
    }
}
pub fn canonicalize<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let name = c_path(path.as_ref())?;
    // SAFETY: name is NUL-terminated; null output requests libc allocation.
    let raw = unsafe { native::realpath(name.as_ptr() as *const core::ffi::c_char, core::ptr::null_mut()) };
    if raw.is_null() { return Err(Error::last_os_error()); }
    let resolved = Resolved(raw);
    // SAFETY: successful realpath returns a terminated path owned by the guard.
    let bytes = unsafe { copy_c_string(resolved.0 as *const u8) };
    Ok(PathBuf::from(OsString { bytes }))
}
