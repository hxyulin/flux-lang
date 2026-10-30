//! Byte-preserving Unix source-file reads and canonical paths.
use crate::alloc_crate::string::String;
use crate::alloc_crate::vec::Vec;
use crate::ffi::OsString;
use crate::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};
use crate::path::{Path, PathBuf};

mod native {
    #[cfg(target_os = "macos")]
    pub type Mode = u16;
    #[cfg(target_os = "linux")]
    pub type Mode = u32;
    unsafe extern "C" {
        pub fn open(path: *const core::ffi::c_char, flags: i32, ...) -> i32;
        pub fn read(fd: i32, buffer: *mut core::ffi::c_void, count: usize) -> isize;
        pub fn close(fd: i32) -> i32;
        pub fn write(fd: i32, buffer: *const core::ffi::c_void, count: usize) -> isize;
        pub fn lseek(fd: i32, offset: i64, whence: i32) -> i64;
        pub fn ftruncate(fd: i32, length: i64) -> i32;
        pub fn fsync(fd: i32) -> i32;
        pub fn fcntl(fd: i32, command: i32, ...) -> i32;
        pub fn fchmod(fd: i32, mode: Mode) -> i32;
        pub fn chmod(path: *const core::ffi::c_char, mode: Mode) -> i32;
        pub fn mkdir(path: *const core::ffi::c_char, mode: Mode) -> i32;
        pub fn rename(from: *const core::ffi::c_char, to: *const core::ffi::c_char) -> i32;
        pub fn unlink(path: *const core::ffi::c_char) -> i32;
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        #[link_name = "stat$INODE64"]
        pub fn metadata_stat(path: *const core::ffi::c_char, buffer: *mut u8) -> i32;
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        #[link_name = "fstat$INODE64"]
        pub fn metadata_fstat(fd: i32, buffer: *mut u8) -> i32;
        #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
        #[link_name = "stat"]
        pub fn metadata_stat(path: *const core::ffi::c_char, buffer: *mut u8) -> i32;
        #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
        #[link_name = "fstat"]
        pub fn metadata_fstat(fd: i32, buffer: *mut u8) -> i32;
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


#[cfg(target_os = "macos")]
const APPEND_FLAG: i32 = 0x8;
#[cfg(target_os = "linux")]
const APPEND_FLAG: i32 = 0x400;
#[cfg(target_os = "macos")]
const CREATE_FLAG: i32 = 0x200;
#[cfg(target_os = "linux")]
const CREATE_FLAG: i32 = 0x40;
#[cfg(target_os = "macos")]
const TRUNCATE_FLAG: i32 = 0x400;
#[cfg(target_os = "linux")]
const TRUNCATE_FLAG: i32 = 0x200;
#[cfg(target_os = "macos")]
const EXCLUSIVE_FLAG: i32 = 0x800;
#[cfg(target_os = "linux")]
const EXCLUSIVE_FLAG: i32 = 0x80;
#[cfg(target_os = "macos")]
const DUPLICATE_CLOEXEC: i32 = 67;
#[cfg(target_os = "linux")]
const DUPLICATE_CLOEXEC: i32 = 1030;

/// An owned operating-system file descriptor, closed exactly once on drop.
pub struct File { descriptor: Descriptor }
impl File {
    pub fn options() -> OpenOptions { OpenOptions::new() }
    pub fn open<P: AsRef<Path>>(path: P) -> Result<File> { OpenOptions::new().read(true).open(path) }
    pub fn create<P: AsRef<Path>>(path: P) -> Result<File> { OpenOptions::new().write(true).create(true).truncate(true).open(path) }
    pub fn create_new<P: AsRef<Path>>(path: P) -> Result<File> { OpenOptions::new().read(true).write(true).create_new(true).open(path) }
    pub(crate) fn raw_fd(&self) -> i32 { self.descriptor.0 }
    pub(crate) unsafe fn from_raw_fd_unchecked(fd: i32) -> Self { Self { descriptor: Descriptor(fd) } }
    pub(crate) fn into_raw_fd_unchecked(self) -> i32 { core::mem::ManuallyDrop::new(self).descriptor.0 }
    fn read_buffer(&self, buffer: &mut [u8]) -> Result<usize> {
        // SAFETY: this owner retains a live descriptor; buffer supplies writable
        // storage of exactly the specified length, bounded to ssize_t::MAX.
        let count=unsafe{native::read(self.raw_fd(),buffer.as_mut_ptr() as *mut core::ffi::c_void,buffer.len().min(isize::MAX as usize))};
        if count<0 {Err(Error::last_os_error())}else{Ok(count as usize)}
    }
    fn write_buffer(&self, buffer: &[u8]) -> Result<usize> {
        // SAFETY: the live descriptor and readable buffer are retained for the
        // call. Unbuffered file writes return their actual short-write count.
        let count=unsafe{native::write(self.raw_fd(),buffer.as_ptr() as *const core::ffi::c_void,buffer.len().min(isize::MAX as usize))};
        if count<0 {Err(Error::last_os_error())}else{Ok(count as usize)}
    }
    fn seek_position(&self,position:SeekFrom)->Result<u64>{
        let (offset,whence)=match position{
            SeekFrom::Start(position)=>{
                if position>i64::MAX as u64{return Err(ErrorKind::InvalidInput.into());}
                (position as i64,0)
            }
            SeekFrom::Current(offset)=>(offset,1),SeekFrom::End(offset)=>(offset,2),
        };
        // SAFETY: fd is live; off_t is signed 64-bit on all admitted Unix targets.
        let result=unsafe{native::lseek(self.raw_fd(),offset,whence)};
        if result<0{Err(Error::last_os_error())}else{Ok(result as u64)}
    }
    pub fn set_len(&self,length:u64)->Result<()>{
        // SAFETY: fd remains owned; a length outside off_t's range becomes a
        // negative offset and the kernel rejects it without modifying the file.
        if unsafe{native::ftruncate(self.raw_fd(),length as i64)}<0{Err(Error::last_os_error())}else{Ok(())}
    }
    pub fn sync_all(&self)->Result<()>{
        // SAFETY: fd remains owned for the complete synchronization call.
        if unsafe{native::fsync(self.raw_fd())}<0{Err(Error::last_os_error())}else{Ok(())}
    }
    pub fn try_clone(&self)->Result<Self>{
        loop{
            // SAFETY: F_DUPFD_CLOEXEC duplicates this live descriptor into a new
            // close-on-exec owner, sharing its open-file description and cursor.
            let fd=unsafe{native::fcntl(self.raw_fd(),DUPLICATE_CLOEXEC,0i32)};
            if fd>=0{return Ok(Self{descriptor:Descriptor(fd)});}
            let error=Error::last_os_error();if error.kind()!=ErrorKind::Interrupted{return Err(error);}
        }
    }
    pub fn metadata(&self)->Result<Metadata>{
        let mut storage=[0u64;32];
        // SAFETY: the aligned buffer exceeds every admitted native stat layout;
        // fstat writes only within that record while this owner keeps fd live.
        if unsafe{native::metadata_fstat(self.raw_fd(),storage.as_mut_ptr() as *mut u8)}<0{return Err(Error::last_os_error());}
        Ok(metadata_record(&storage))
    }
    pub fn set_permissions(&self,permissions:Permissions)->Result<()>{
        // SAFETY: fd is live; native mode_t receives its exact platform width.
        if unsafe{native::fchmod(self.raw_fd(),permissions.mode as native::Mode)}<0{Err(Error::last_os_error())}else{Ok(())}
    }
}
impl core::fmt::Debug for File{
    fn fmt(&self,f:&mut core::fmt::Formatter<'_>)->core::fmt::Result{f.debug_struct("File").field("fd",&self.raw_fd()).finish()}
}
impl Read for File{fn read(&mut self,buffer:&mut[u8])->Result<usize>{self.read_buffer(buffer)}}
impl Read for &File{fn read(&mut self,buffer:&mut[u8])->Result<usize>{self.read_buffer(buffer)}}
impl Write for File{
    fn write(&mut self,buffer:&[u8])->Result<usize>{self.write_buffer(buffer)}
    // File has no userspace buffering; durability is provided by sync_all.
    fn flush(&mut self)->Result<()>{Ok(())}
}
impl Write for &File{
    fn write(&mut self,buffer:&[u8])->Result<usize>{self.write_buffer(buffer)}
    fn flush(&mut self)->Result<()>{Ok(())}
}
impl Seek for File{fn seek(&mut self,position:SeekFrom)->Result<u64>{self.seek_position(position)}}
impl Seek for &File{fn seek(&mut self,position:SeekFrom)->Result<u64>{self.seek_position(position)}}

/// Options applied atomically when opening a file.
#[derive(Clone,Debug)]
pub struct OpenOptions { read:bool, write:bool, append:bool, truncate:bool, create:bool, create_new:bool }
impl OpenOptions{
    pub fn new()->Self{Self{read:false,write:false,append:false,truncate:false,create:false,create_new:false}}
    pub fn read(&mut self,enabled:bool)->&mut Self{self.read=enabled;self}
    pub fn write(&mut self,enabled:bool)->&mut Self{self.write=enabled;self}
    pub fn append(&mut self,enabled:bool)->&mut Self{self.append=enabled;self}
    pub fn truncate(&mut self,enabled:bool)->&mut Self{self.truncate=enabled;self}
    pub fn create(&mut self,enabled:bool)->&mut Self{self.create=enabled;self}
    pub fn create_new(&mut self,enabled:bool)->&mut Self{self.create_new=enabled;self}
    pub fn open<P:AsRef<Path>>(&self,path:P)->Result<File>{
        if !(self.read||self.write||self.append)
            || (!(self.write||self.append)&&(self.create||self.create_new||self.truncate))
            || (self.append&&self.truncate&&!self.create_new){return Err(ErrorKind::InvalidInput.into());}
        let mut flags=READ_FLAGS;
        flags|=if self.read&&(self.write||self.append){2}else if self.write||self.append{1}else{0};
        if self.append{flags|=APPEND_FLAG;}
        if self.create_new{flags|=CREATE_FLAG|EXCLUSIVE_FLAG;}else{
            if self.create{flags|=CREATE_FLAG;}
            if self.truncate{flags|=TRUNCATE_FLAG;}
        }
        let name=c_path(path.as_ref())?;
        loop{
            // SAFETY: path is NUL-terminated, flags select a checked access
            // mode, and variadic mode uses C's promoted unsigned-int width.
            let fd=unsafe{native::open(name.as_ptr() as *const core::ffi::c_char,flags,0o666u32)};
            if fd>=0{return Ok(File{descriptor:Descriptor(fd)});}
            let error=Error::last_os_error();if error.kind()!=ErrorKind::Interrupted{return Err(error);}
        }
    }
}
impl Default for OpenOptions{fn default()->Self{Self::new()}}

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Permissions { pub(crate) mode:u32 }
impl Permissions{
    pub fn readonly(&self)->bool{self.mode&0o222==0}
    pub fn set_readonly(&mut self,readonly:bool){if readonly{self.mode&=!0o222;}else{self.mode|=0o222;}}
}
#[derive(Clone,Debug)]
pub struct Metadata{mode:u32,len:u64,pub(crate) native:[u64;32]}
impl Metadata{
    pub fn len(&self)->u64{self.len}
    pub fn is_file(&self)->bool{self.mode&0xf000==0x8000}
    pub fn is_dir(&self)->bool{self.mode&0xf000==0x4000}
    pub fn permissions(&self)->Permissions{Permissions{mode:self.mode}}
    pub fn file_type(&self)->FileType{FileType{mode:self.mode}}
}
fn metadata_record(storage:&[u64;32])->Metadata{
    let pointer=storage.as_ptr() as *const u8;
    #[cfg(target_os="macos")]
    // SAFETY: Darwin inode64 stat's st_mode is u16 at4 and st_size i64 at96.
    let (mode,len)=unsafe{(*(pointer.add(4) as *const u16) as u32,*(pointer.add(96) as *const i64) as u64)};
    #[cfg(all(target_os="linux",target_arch="x86_64"))]
    // SAFETY: native Linux x86-64 stat stores mode at24 and size at48.
    let (mode,len)=unsafe{(*(pointer.add(24) as *const u32),*(pointer.add(48) as *const i64) as u64)};
    #[cfg(all(target_os="linux",target_arch="aarch64"))]
    // SAFETY: native Linux AArch64 stat stores mode at16 and size at48.
    let (mode,len)=unsafe{(*(pointer.add(16) as *const u32),*(pointer.add(48) as *const i64) as u64)};
    Metadata{mode,len,native:*storage}
}
pub fn metadata<P:AsRef<Path>>(path:P)->Result<Metadata>{
    let name=c_path(path.as_ref())?;let mut storage=[0u64;32];
    // SAFETY: path is NUL-terminated; aligned storage exceeds the native record.
    if unsafe{native::metadata_stat(name.as_ptr() as *const core::ffi::c_char,storage.as_mut_ptr() as *mut u8)}<0{return Err(Error::last_os_error());}
    Ok(metadata_record(&storage))
}
pub fn set_permissions<P:AsRef<Path>>(path:P,permissions:Permissions)->Result<()>{
    let name=c_path(path.as_ref())?;
    // SAFETY: path is NUL-terminated and mode has native mode_t width.
    if unsafe{native::chmod(name.as_ptr() as *const core::ffi::c_char,permissions.mode as native::Mode)}<0{Err(Error::last_os_error())}else{Ok(())}
}
pub fn write<P:AsRef<Path>,C:AsRef<[u8]>>(path:P,contents:C)->Result<()>{File::create(path)?.write_all(contents.as_ref())}
pub fn create_dir<P:AsRef<Path>>(path:P)->Result<()>{
    let name=c_path(path.as_ref())?;
    // SAFETY: path is NUL-terminated and 0777 is passed as native mode_t.
    if unsafe{native::mkdir(name.as_ptr() as *const core::ffi::c_char,0o777 as native::Mode)}<0{Err(Error::last_os_error())}else{Ok(())}
}
pub fn create_dir_all<P:AsRef<Path>>(path:P)->Result<()>{create_directories(path.as_ref())}
fn create_directories(path:&Path)->Result<()>{
    if path.as_os_str().is_empty(){return Ok(());}
    let error=match create_dir(path){Ok(())=>return Ok(()),Err(error)=>error};
    if metadata(path).map(|metadata|metadata.is_dir()).unwrap_or(false){return Ok(());}
    if error.kind()!=ErrorKind::NotFound{return Err(error);}
    if let Some(parent)=path.parent(){create_directories(parent)?;}
    match create_dir(path){
        Ok(())=>Ok(()),
        Err(error)=>if metadata(path).map(|metadata|metadata.is_dir()).unwrap_or(false){Ok(())}else{Err(error)},
    }
}
pub fn rename<P:AsRef<Path>,Q:AsRef<Path>>(from:P,to:Q)->Result<()>{
    let from=c_path(from.as_ref())?;let to=c_path(to.as_ref())?;
    // SAFETY: both byte paths are retained NUL-terminated buffers. POSIX rename
    // performs replacement atomically without exposing an incomplete new file.
    if unsafe{native::rename(from.as_ptr() as *const core::ffi::c_char,to.as_ptr() as *const core::ffi::c_char)}<0{Err(Error::last_os_error())}else{Ok(())}
}
pub fn remove_file<P:AsRef<Path>>(path:P)->Result<()>{
    let name=c_path(path.as_ref())?;
    // SAFETY: the NUL-terminated path remains live; unlink never follows a
    // final symlink and does not remove an existing open file description.
    if unsafe{native::unlink(name.as_ptr() as *const core::ffi::c_char)}<0{Err(Error::last_os_error())}else{Ok(())}
}

/// The kind of a directory entry, without following symbolic links.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileType { mode: u32 }
impl FileType {
    pub fn is_file(&self) -> bool { self.mode & 0xf000 == 0x8000 }
    pub fn is_dir(&self) -> bool { self.mode & 0xf000 == 0x4000 }
    pub fn is_symlink(&self) -> bool { self.mode & 0xf000 == 0xa000 }
}

/// Reads metadata for the link itself rather than its destination.
pub fn symlink_metadata<P: AsRef<Path>>(path: P) -> Result<Metadata> {
    unsafe extern "C" {
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        #[link_name = "lstat$INODE64"]
        fn link_stat(path: *const u8, storage: *mut u64) -> i32;
        #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
        #[link_name = "lstat"]
        fn link_stat(path: *const u8, storage: *mut u64) -> i32;
    }
    let path = c_path(path.as_ref())?;
    let mut storage = [0u64; 32];
    // SAFETY: aligned storage exceeds the supported native stat records;
    // path is terminated and remains live throughout the call.
    if unsafe { link_stat(path.as_ptr(), storage.as_mut_ptr()) } < 0 {
        return Err(Error::last_os_error());
    }
    Ok(metadata_record(&storage))
}

/// An owned directory entry; it remains usable after its iterator is dropped.
#[derive(Debug)]
pub struct DirEntry { path: PathBuf, name: OsString }
impl DirEntry {
    pub fn path(&self) -> PathBuf { self.path.clone() }
    pub fn file_name(&self) -> OsString { self.name.clone() }
    pub fn metadata(&self) -> Result<Metadata> { symlink_metadata(&self.path) }
    pub fn file_type(&self) -> Result<FileType> { Ok(self.metadata()?.file_type()) }
}

unsafe extern "C" {
    fn opendir(path: *const u8) -> *mut core::ffi::c_void;
    fn closedir(directory: *mut core::ffi::c_void) -> i32;
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    #[link_name = "readdir$INODE64"]
    fn directory_entry(directory: *mut core::ffi::c_void) -> *const u8;
    #[cfg(not(all(target_os = "macos", target_arch = "x86_64")))]
    #[link_name = "readdir"]
    fn directory_entry(directory: *mut core::ffi::c_void) -> *const u8;
    #[cfg(target_os = "macos")]
    fn __error() -> *mut i32;
    #[cfg(target_os = "linux")]
    fn __errno_location() -> *mut i32;
}

/// A directory stream. Entries copy native names before advancing the stream.
pub struct ReadDir { directory: *mut core::ffi::c_void, path: PathBuf, done: bool }
impl Drop for ReadDir {
    fn drop(&mut self) {
        // SAFETY: this iterator owns the successful opendir result exclusively.
        unsafe { closedir(self.directory); }
    }
}
impl core::fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReadDir").field("path", &self.path).finish()
    }
}
impl Iterator for ReadDir {
    type Item = Result<DirEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done { return None; }
        loop {
            // SAFETY: libc provides the current thread's writable errno slot.
            // readdir needs errno cleared to distinguish end-of-stream from error.
            unsafe {
                #[cfg(target_os = "macos")]
                { *__error() = 0; }
                #[cfg(target_os = "linux")]
                { *__errno_location() = 0; }
            }
            // SAFETY: this iterator owns a live directory stream; the entry
            // remains readable until the next call on this stream or its drop.
            let entry = unsafe { directory_entry(self.directory) };
            if entry.is_null() {
                let error = Error::last_os_error();
                self.done = true;
                return if error.raw_os_error() == Some(0) { None } else { Some(Err(error)) };
            }
            // Darwin inode64 and Linux dirent64 both store reclen at byte 16.
            // Their names start at byte 21 and 19 respectively. Read only the
            // returned record and copy the name before calling readdir again.
            #[cfg(target_os = "macos")]
            let start = 21usize;
            #[cfg(target_os = "linux")]
            let start = 19usize;
            // SAFETY: the native directory record has its fixed header and
            // d_reclen bytes of readable storage, including the terminated name.
            let name = unsafe {
                let length = *(entry.add(16) as *const u16) as usize;
                let bytes = core::slice::from_raw_parts(entry.add(start), length - start);
                let end = bytes.iter().position(|&b| b == 0).unwrap();
                OsString { bytes: bytes[..end].to_vec() }
            };
            if name.as_os_str() == crate::ffi::OsStr::new(".") || name.as_os_str() == crate::ffi::OsStr::new("..") { continue; }
            return Some(Ok(DirEntry { path: self.path.join(&name), name }));
        }
    }
}

/// Opens a directory for iteration, retaining non-Unicode entry names.
pub fn read_dir<P: AsRef<Path>>(path: P) -> Result<ReadDir> {
    let path = path.as_ref();
    let name = c_path(path)?;
    // SAFETY: name is terminated and remains live for the synchronous open.
    let directory = unsafe { opendir(name.as_ptr()) };
    if directory.is_null() { return Err(Error::last_os_error()); }
    Ok(ReadDir { directory, path: path.to_path_buf(), done: false })
}

/// Creates a symbolic link; the destination need not exist.
pub(crate) fn symlink(original: &Path, link: &Path) -> Result<()> {
    unsafe extern "C" { fn symlink(original: *const u8, link: *const u8) -> i32; }
    let original = c_path(original)?;
    let link = c_path(link)?;
    // SAFETY: both byte paths are terminated and retained throughout the call.
    if unsafe { symlink(original.as_ptr(), link.as_ptr()) } < 0 { Err(Error::last_os_error()) } else { Ok(()) }
}

/// Removes a directory tree without traversing symbolic links.
#[cfg(target_os = "macos")]
pub fn remove_dir_all<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let kind = symlink_metadata(path)?.file_type();
    if kind.is_symlink() { return remove_file(path); }
    if !kind.is_dir() { return Err(Error::from_raw_os_error(20)); }
    unsafe extern "C" { fn removefile(path: *const u8, state: *mut core::ffi::c_void, flags: u32) -> i32; }
    let name = c_path(path)?;
    // SAFETY: name is terminated. Darwin's recursive removal uses physical
    // traversal; no callback/state is supplied and symbolic links are not followed.
    if unsafe { removefile(name.as_ptr(), core::ptr::null_mut(), 1) } < 0 { Err(Error::last_os_error()) } else { Ok(()) }
}
