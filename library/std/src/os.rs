//! Unix descriptor ownership and filesystem permission extensions.
pub mod unix {
    /// The Unix extension traits most code imports together.
    pub mod prelude {
        pub use super::ffi::{OsStrExt, OsStringExt};
        pub use super::fs::{DirEntryExt, FileExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
        pub use super::io::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
    }
    /// Native pthread handles of spawned threads.
    pub mod thread {
        /// libc's pthread_t.
        #[cfg(target_os = "macos")]
        pub type RawPthread = usize;
        #[cfg(target_os = "linux")]
        pub type RawPthread = u64;
        pub trait JoinHandleExt {
            fn as_pthread_t(&self) -> RawPthread;
            fn into_pthread_t(self) -> RawPthread;
        }
        impl<T> JoinHandleExt for crate::thread::JoinHandle<T> {
            fn as_pthread_t(&self) -> RawPthread { self.native_thread() as RawPthread }
            fn into_pthread_t(self) -> RawPthread { self.into_native_thread() as RawPthread }
        }
    }
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
        /// Unix file kinds beyond files, directories and links.
        pub trait FileTypeExt {
            fn is_block_device(&self) -> bool;
            fn is_char_device(&self) -> bool;
            fn is_fifo(&self) -> bool;
            fn is_socket(&self) -> bool;
        }
        impl FileTypeExt for crate::fs::FileType {
            fn is_block_device(&self) -> bool { self.mode & 0xf000 == 0x6000 }
            fn is_char_device(&self) -> bool { self.mode & 0xf000 == 0x2000 }
            fn is_fifo(&self) -> bool { self.mode & 0xf000 == 0x1000 }
            fn is_socket(&self) -> bool { self.mode & 0xf000 == 0xc000 }
        }
        /// The inode number of a directory entry.
        pub trait DirEntryExt {
            fn ino(&self) -> u64;
        }
        impl DirEntryExt for crate::fs::DirEntry {
            fn ino(&self) -> u64 { self.ino }
        }
        /// Reads and writes at an offset, without moving the file cursor.
        pub trait FileExt {
            fn read_at(&self, buf: &mut [u8], offset: u64) -> crate::io::Result<usize>;
            fn write_at(&self, buf: &[u8], offset: u64) -> crate::io::Result<usize>;
            fn read_exact_at(&self, mut buf: &mut [u8], mut offset: u64) -> crate::io::Result<()> {
                while !buf.is_empty() {
                    match self.read_at(buf, offset) {
                        Ok(0) => break,
                        Ok(n) => { let tmp = buf; buf = &mut tmp[n..]; offset += n as u64; }
                        Err(ref e) if e.kind() == crate::io::ErrorKind::Interrupted => {}
                        Err(e) => return Err(e),
                    }
                }
                if !buf.is_empty() {
                    Err(crate::io::Error::new(crate::io::ErrorKind::UnexpectedEof, "failed to fill whole buffer"))
                } else {
                    Ok(())
                }
            }
            fn write_all_at(&self, mut buf: &[u8], mut offset: u64) -> crate::io::Result<()> {
                while !buf.is_empty() {
                    match self.write_at(buf, offset) {
                        Ok(0) => return Err(crate::io::Error::new(crate::io::ErrorKind::WriteZero, "failed to write whole buffer")),
                        Ok(n) => { buf = &buf[n..]; offset += n as u64 }
                        Err(ref e) if e.kind() == crate::io::ErrorKind::Interrupted => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
        }
        impl FileExt for crate::fs::File {
            fn read_at(&self, buf: &mut [u8], offset: u64) -> crate::io::Result<usize> {
                unsafe extern "C" { fn pread(fd: i32, buf: *mut u8, count: usize, offset: i64) -> isize; }
                use super::io::AsRawFd;
                // SAFETY: the descriptor is open and buf is writable for its length.
                let count = unsafe { pread(self.as_raw_fd(), buf.as_mut_ptr(), buf.len().min(isize::MAX as usize), offset as i64) };
                if count < 0 { Err(crate::io::Error::last_os_error()) } else { Ok(count as usize) }
            }
            fn write_at(&self, buf: &[u8], offset: u64) -> crate::io::Result<usize> {
                unsafe extern "C" { fn pwrite(fd: i32, buf: *const u8, count: usize, offset: i64) -> isize; }
                use super::io::AsRawFd;
                // SAFETY: the descriptor is open and buf is readable for its length.
                let count = unsafe { pwrite(self.as_raw_fd(), buf.as_ptr(), buf.len().min(isize::MAX as usize), offset as i64) };
                if count < 0 { Err(crate::io::Error::last_os_error()) } else { Ok(count as usize) }
            }
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
        pub use crate::os::fd::{AsFd, BorrowedFd, OwnedFd};
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
/// Owned and borrowed Unix file descriptors.
pub mod fd {
    pub use super::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
    use core::marker::PhantomData;

    /// A descriptor borrowed for `'fd`; it is not closed on drop.
    #[derive(Copy, Clone)]
    #[repr(transparent)]
    pub struct BorrowedFd<'fd> { fd: RawFd, owner: PhantomData<&'fd OwnedFd> }
    /// An owned descriptor, closed exactly once on drop.
    #[repr(transparent)]
    pub struct OwnedFd { fd: RawFd }

    impl BorrowedFd<'_> {
        /// # Safety
        /// `fd` must stay open for the returned lifetime and must not be -1.
        pub const unsafe fn borrow_raw(fd: RawFd) -> Self {
            assert!(fd != u32::MAX as RawFd);
            BorrowedFd { fd, owner: PhantomData }
        }
        /// Duplicates the descriptor, with close-on-exec set.
        pub fn try_clone_to_owned(&self) -> crate::io::Result<OwnedFd> {
            unsafe extern "C" { fn fcntl(fd: i32, command: i32, ...) -> i32; }
            #[cfg(target_os = "macos")]
            const DUPLICATE_CLOEXEC: i32 = 67;
            #[cfg(target_os = "linux")]
            const DUPLICATE_CLOEXEC: i32 = 1030;
            // SAFETY: the borrowed descriptor is open; fcntl duplicates it.
            let fd = unsafe { fcntl(self.fd, DUPLICATE_CLOEXEC, 3i32) };
            if fd < 0 { Err(crate::io::Error::last_os_error()) } else { Ok(OwnedFd { fd }) }
        }
    }
    impl OwnedFd {
        pub fn try_clone(&self) -> crate::io::Result<OwnedFd> { self.as_fd().try_clone_to_owned() }
    }
    impl Drop for OwnedFd {
        fn drop(&mut self) {
            unsafe extern "C" { fn close(fd: i32) -> i32; }
            // SAFETY: this owner holds the descriptor and closes it once.
            // Errors are ignored, as std's destructor ignores them.
            unsafe { close(self.fd); }
        }
    }
    impl AsRawFd for BorrowedFd<'_> { fn as_raw_fd(&self) -> RawFd { self.fd } }
    impl AsRawFd for OwnedFd { fn as_raw_fd(&self) -> RawFd { self.fd } }
    impl AsRawFd for RawFd { fn as_raw_fd(&self) -> RawFd { *self } }
    impl IntoRawFd for OwnedFd {
        fn into_raw_fd(self) -> RawFd { core::mem::ManuallyDrop::new(self).fd }
    }
    impl IntoRawFd for RawFd { fn into_raw_fd(self) -> RawFd { self } }
    impl FromRawFd for OwnedFd {
        /// # Safety
        /// `fd` must be open and owned by the caller, and not -1.
        unsafe fn from_raw_fd(fd: RawFd) -> Self {
            assert_ne!(fd, u32::MAX as RawFd);
            OwnedFd { fd }
        }
    }
    impl FromRawFd for RawFd { unsafe fn from_raw_fd(fd: RawFd) -> RawFd { fd } }
    impl core::fmt::Debug for BorrowedFd<'_> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("BorrowedFd").field("fd", &self.fd).finish() }
    }
    impl core::fmt::Debug for OwnedFd {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("OwnedFd").field("fd", &self.fd).finish() }
    }

    /// Borrows a value's descriptor.
    pub trait AsFd {
        fn as_fd(&self) -> BorrowedFd<'_>;
    }
    impl<T: AsFd + ?Sized> AsFd for &T { fn as_fd(&self) -> BorrowedFd<'_> { T::as_fd(self) } }
    impl<T: AsFd + ?Sized> AsFd for &mut T { fn as_fd(&self) -> BorrowedFd<'_> { T::as_fd(self) } }
    impl<T: AsFd + ?Sized> AsFd for crate::boxed::Box<T> { fn as_fd(&self) -> BorrowedFd<'_> { (**self).as_fd() } }
    impl<T: AsFd + ?Sized> AsFd for crate::rc::Rc<T> { fn as_fd(&self) -> BorrowedFd<'_> { (**self).as_fd() } }
    impl<T: AsFd + ?Sized> AsFd for crate::sync::Arc<T> { fn as_fd(&self) -> BorrowedFd<'_> { (**self).as_fd() } }
    impl AsFd for BorrowedFd<'_> { fn as_fd(&self) -> BorrowedFd<'_> { *self } }
    impl AsFd for OwnedFd {
        // SAFETY: the owner keeps the descriptor open for the borrow.
        fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(self.fd) } }
    }
    macro_rules! as_fd_by_raw {
        ($($t:ty),*) => {$(
            impl AsFd for $t {
                // SAFETY: the value keeps its descriptor open while borrowed.
                fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) } }
            }
        )*};
    }
    as_fd_by_raw!(crate::fs::File, crate::io::Stdin, crate::io::Stdout, crate::io::Stderr);
    impl<'a> AsFd for crate::io::StdinLock<'a> {
        // SAFETY: standard input stays open for the process.
        fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(0) } }
    }
    impl<'a> AsFd for crate::io::StdoutLock<'a> {
        // SAFETY: standard output stays open for the process.
        fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(1) } }
    }
    impl<'a> AsFd for crate::io::StderrLock<'a> {
        // SAFETY: standard error stays open for the process.
        fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(2) } }
    }
    impl From<crate::fs::File> for OwnedFd {
        fn from(file: crate::fs::File) -> OwnedFd { OwnedFd { fd: file.into_raw_fd() } }
    }
    impl From<OwnedFd> for crate::fs::File {
        fn from(fd: OwnedFd) -> crate::fs::File {
            // SAFETY: the OwnedFd transfers sole ownership of an open descriptor.
            unsafe { crate::fs::File::from_raw_fd(fd.into_raw_fd()) }
        }
    }
}

/// C type aliases, as std::ffi names them.
pub mod raw {
    pub use core::ffi::{
        c_char, c_double, c_float, c_int, c_long, c_longlong, c_schar, c_short, c_uchar, c_uint, c_ulong,
        c_ulonglong, c_ushort, c_void,
    };
}
