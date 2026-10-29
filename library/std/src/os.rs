//! Unix descriptor ownership and filesystem permission extensions.
pub mod unix {
    #[cfg(target_os = "macos")]
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
