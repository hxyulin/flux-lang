//! Unix descriptor ownership and filesystem permission extensions.
pub mod unix {
    #[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
    pub mod process { pub use crate::process::command::ExitStatusExt; }
    pub mod ffi {
        mod private {
            pub trait Sealed {}
            impl Sealed for crate::ffi::OsStr {}
            impl Sealed for crate::ffi::OsString {}
        }
        pub trait OsStrExt: private::Sealed {
            fn from_bytes(bytes: &[u8]) -> &Self;
            fn as_bytes(&self) -> &[u8];
        }
        impl OsStrExt for crate::ffi::OsStr {
            fn from_bytes(bytes: &[u8]) -> &Self {
                // SAFETY: every byte sequence is a valid Unix OS string, and
                // the returned shared view borrows exactly the input slice.
                unsafe { Self::from_encoded_bytes_unchecked(bytes) }
            }
            fn as_bytes(&self) -> &[u8] {
                self.as_encoded_bytes()
            }
        }
        pub trait OsStringExt: private::Sealed {
            fn from_vec(bytes: crate::alloc_crate::vec::Vec<u8>) -> Self;
            fn into_vec(self) -> crate::alloc_crate::vec::Vec<u8>;
        }
        impl OsStringExt for crate::ffi::OsString {
            fn from_vec(bytes: crate::alloc_crate::vec::Vec<u8>) -> Self {
                Self { bytes }
            }
            fn into_vec(self) -> crate::alloc_crate::vec::Vec<u8> {
                self.bytes
            }
        }
    }
    pub mod fs {
        /// Unix permission bits and open(2) flags for OpenOptions.
        pub trait OpenOptionsExt {
            fn mode(&mut self, mode: u32) -> &mut Self;
            fn custom_flags(&mut self, flags: i32) -> &mut Self;
        }
        impl OpenOptionsExt for crate::fs::OpenOptions {
            fn mode(&mut self, mode: u32) -> &mut Self { self.mode = mode; self }
            fn custom_flags(&mut self, flags: i32) -> &mut Self { self.custom_flags = flags; self }
        }
        pub fn symlink<P: AsRef<crate::path::Path>, Q: AsRef<crate::path::Path>>(original: P, link: Q) -> crate::io::Result<()> {
            crate::fs::symlink(original.as_ref(), link.as_ref())
        }
        pub trait MetadataExt {
            fn dev(&self) -> u64;
            fn ino(&self) -> u64;
            fn mtime(&self) -> i64;
            fn mtime_nsec(&self) -> i64;
            fn ctime(&self) -> i64;
            fn ctime_nsec(&self) -> i64;
            fn mode(&self) -> u32;
            fn nlink(&self) -> u64;
            fn uid(&self) -> u32;
            fn gid(&self) -> u32;
            fn rdev(&self) -> u64;
            fn size(&self) -> u64;
            fn atime(&self) -> i64;
            fn atime_nsec(&self) -> i64;
            fn blksize(&self) -> u64;
            fn blocks(&self) -> u64;
        }
        impl MetadataExt for crate::fs::Metadata {
            fn dev(&self) -> u64 {
                #[cfg(target_os = "macos")]
                { self.native[0] as u32 as u64 }
                #[cfg(target_os = "linux")]
                { self.native[0] }
            }
            fn ino(&self) -> u64 { self.native[1] }
            fn mtime(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[6] as i64 }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                { self.native[11] as i64 }
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                { self.native[11] as i64 }
            }
            fn mtime_nsec(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[7] as i64 }
                #[cfg(target_os = "linux")]
                { self.native[12] as i64 }
            }
            fn ctime(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[8] as i64 }
                #[cfg(target_os = "linux")]
                { self.native[13] as i64 }
            }
            fn ctime_nsec(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[9] as i64 }
                #[cfg(target_os = "linux")]
                { self.native[14] as i64 }
            }
            fn mode(&self) -> u32 { self.permissions().mode }
            fn nlink(&self) -> u64 {
                #[cfg(target_os = "macos")]
                { (self.native[0] >> 48) as u16 as u64 }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                { self.native[2] }
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                { (self.native[2] >> 32) as u32 as u64 }
            }
            fn uid(&self) -> u32 {
                #[cfg(target_os = "macos")]
                { self.native[2] as u32 }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                { (self.native[3] >> 32) as u32 }
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                { self.native[3] as u32 }
            }
            fn gid(&self) -> u32 {
                #[cfg(target_os = "macos")]
                { (self.native[2] >> 32) as u32 }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                { self.native[4] as u32 }
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                { (self.native[3] >> 32) as u32 }
            }
            fn rdev(&self) -> u64 {
                #[cfg(target_os = "macos")]
                { self.native[3] as u32 as u64 }
                #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                { self.native[5] }
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                { self.native[4] }
            }
            fn size(&self) -> u64 { self.len() }
            fn atime(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[4] as i64 }
                #[cfg(target_os = "linux")]
                { self.native[9] as i64 }
            }
            fn atime_nsec(&self) -> i64 {
                #[cfg(target_os = "macos")]
                { self.native[5] as i64 }
                #[cfg(target_os = "linux")]
                { self.native[10] as i64 }
            }
            fn blksize(&self) -> u64 {
                #[cfg(target_os = "macos")]
                { self.native[14] as u32 as u64 }
                #[cfg(target_os = "linux")]
                { self.native[7] as u32 as u64 }
            }
            fn blocks(&self) -> u64 {
                #[cfg(target_os = "macos")]
                { self.native[13] }
                #[cfg(target_os = "linux")]
                { self.native[8] }
            }
        }

        pub trait PermissionsExt {
            fn mode(&self) -> u32;
            fn set_mode(&mut self, mode: u32);
            fn from_mode(mode: u32) -> Self;
        }
        impl PermissionsExt for crate::fs::Permissions {
            fn mode(&self) -> u32 {
                self.mode
            }
            fn set_mode(&mut self, mode: u32) {
                self.mode = mode;
            }
            fn from_mode(mode: u32) -> Self {
                Self { mode }
            }
        }
    }
    pub mod io {
        pub type RawFd = i32;
        pub trait AsRawFd {
            fn as_raw_fd(&self) -> RawFd;
        }
        pub trait IntoRawFd {
            fn into_raw_fd(self) -> RawFd;
        }
        pub trait FromRawFd {
            unsafe fn from_raw_fd(fd: RawFd) -> Self;
        }
        impl AsRawFd for crate::fs::File {
            fn as_raw_fd(&self) -> RawFd {
                self.raw_fd()
            }
        }
        impl IntoRawFd for crate::fs::File {
            fn into_raw_fd(self) -> RawFd {
                self.into_raw_fd_unchecked()
            }
        }
        impl FromRawFd for crate::fs::File {
            unsafe fn from_raw_fd(fd: RawFd) -> Self {
                // SAFETY: this forwards the caller's valid unique-owner contract.
                unsafe { Self::from_raw_fd_unchecked(fd) }
            }
        }
        impl<T: AsRawFd + ?Sized> AsRawFd for &T {
            fn as_raw_fd(&self) -> RawFd {
                (**self).as_raw_fd()
            }
        }
        impl<T: AsRawFd + ?Sized> AsRawFd for &mut T {
            fn as_raw_fd(&self) -> RawFd {
                (**self).as_raw_fd()
            }
        }
    }
}
pub mod fd {
    pub use super::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
}
