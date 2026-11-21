//! Anonymous pipes backed by native file descriptors.
use super::{Error, Read, Result, Write};
use crate::fs::File;
use crate::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};

mod native {
    unsafe extern "C" {
        #[cfg(target_os = "macos")]
        pub fn pipe(fds: *mut i32) -> i32;
        #[cfg(target_os = "macos")]
        pub fn fcntl(fd: i32, command: i32, ...) -> i32;
        #[cfg(target_os = "linux")]
        pub fn pipe2(fds: *mut i32, flags: i32) -> i32;
    }
}

pub(crate) fn pipe_files() -> Result<(File, File)> {
    let mut descriptors = [-1; 2];
    // SAFETY: both calls write into two valid descriptor slots.
    #[cfg(target_os = "macos")]
    let status = unsafe { native::pipe(descriptors.as_mut_ptr()) };
    #[cfg(target_os = "linux")]
    let status = unsafe { native::pipe2(descriptors.as_mut_ptr(), 0x80000) }; // O_CLOEXEC
    if status < 0 {
        return Err(Error::last_os_error());
    }
    // SAFETY: the successful call returned two distinct owned descriptors.
    let (reader, writer) = unsafe {
        (
            File::from_raw_fd(descriptors[0]),
            File::from_raw_fd(descriptors[1]),
        )
    };
    #[cfg(target_os = "macos")]
    for fd in descriptors {
        // SAFETY: both owners are live. F_SETFD takes an integer flag.
        if unsafe { native::fcntl(fd, 2, 1i32) } < 0 {
            return Err(Error::last_os_error());
        }
    }
    Ok((reader, writer))
}

/// Creates a connected reader and writer for an anonymous pipe.
pub fn pipe() -> Result<(PipeReader, PipeWriter)> {
    let (reader, writer) = pipe_files()?;
    Ok((PipeReader(reader), PipeWriter(writer)))
}

/// The reading end of an anonymous pipe.
#[derive(Debug)]
pub struct PipeReader(File);

/// The writing end of an anonymous pipe.
#[derive(Debug)]
pub struct PipeWriter(File);

macro_rules! pipe_end {
    ($name:ident) => {
        impl $name {
            /// Duplicates this end without creating a second pipe.
            pub fn try_clone(&self) -> Result<Self> {
                Ok(Self(self.0.try_clone()?))
            }
        }
        impl AsFd for $name {
            fn as_fd(&self) -> BorrowedFd<'_> {
                self.0.as_fd()
            }
        }
        impl AsRawFd for $name {
            fn as_raw_fd(&self) -> i32 {
                self.0.as_raw_fd()
            }
        }
        impl IntoRawFd for $name {
            fn into_raw_fd(self) -> i32 {
                self.0.into_raw_fd()
            }
        }
        impl FromRawFd for $name {
            unsafe fn from_raw_fd(fd: i32) -> Self {
                // SAFETY: the caller transfers a live, uniquely owned pipe end.
                Self(unsafe { File::from_raw_fd(fd) })
            }
        }
        impl From<$name> for OwnedFd {
            fn from(value: $name) -> Self {
                value.0.into()
            }
        }
        impl From<OwnedFd> for $name {
            fn from(fd: OwnedFd) -> Self {
                Self(fd.into())
            }
        }
    };
}
pipe_end!(PipeReader);
pipe_end!(PipeWriter);

impl Read for &PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        (&self.0).read(buffer)
    }
}
impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        (&*self).read(buffer)
    }
}
impl Write for &PipeWriter {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        (&self.0).write(buffer)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
impl Write for PipeWriter {
    fn write(&mut self, buffer: &[u8]) -> Result<usize> {
        (&*self).write(buffer)
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
