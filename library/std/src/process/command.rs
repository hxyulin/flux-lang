//! Native Darwin process creation with owned pipe endpoints.
use crate::alloc_crate::collections::BTreeMap;
use crate::alloc_crate::vec::Vec;
use crate::ffi::{CString, OsStr, OsString};
use crate::fs::File;
use crate::io::{self, Error, ErrorKind, Read, Write};
use crate::path::{Path, PathBuf};
use core::ffi::{c_char, c_void};

mod native {
    use super::{c_char, c_void};
    #[repr(C)]
    pub struct PollFd { pub fd: i32, pub events: i16, pub revents: i16 }
    unsafe extern "C" {
        pub fn pipe(descriptors: *mut i32) -> i32;
        pub fn fcntl(descriptor: i32, command: i32, ...) -> i32;
        pub fn poll(descriptors: *mut PollFd, count: u32, timeout: i32) -> i32;
        pub fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        pub fn kill(pid: i32, signal: i32) -> i32;
        pub fn posix_spawn(pid: *mut i32, path: *const c_char, actions: *const *mut c_void, attributes: *const *mut c_void, arguments: *const *mut c_char, environment: *const *mut c_char) -> i32;
        pub fn posix_spawn_file_actions_init(actions: *mut *mut c_void) -> i32;
        pub fn posix_spawn_file_actions_destroy(actions: *mut *mut c_void) -> i32;
        pub fn posix_spawn_file_actions_adddup2(actions: *mut *mut c_void, source: i32, target: i32) -> i32;
        pub fn posix_spawn_file_actions_addinherit_np(actions: *mut *mut c_void, descriptor: i32) -> i32;
        pub fn posix_spawn_file_actions_addchdir_np(actions: *mut *mut c_void, path: *const c_char) -> i32;
        pub fn posix_spawnattr_init(attributes: *mut *mut c_void) -> i32;
        pub fn posix_spawnattr_destroy(attributes: *mut *mut c_void) -> i32;
        pub fn posix_spawnattr_setflags(attributes: *mut *mut c_void, flags: i16) -> i32;
        pub fn posix_spawnattr_setsigdefault(attributes: *mut *mut c_void, signals: *const u32) -> i32;
    }
}
fn error_code(code: i32) -> io::Result<()> { if code == 0 { Ok(()) } else { Err(Error::from_raw_os_error(code)) } }
fn c_string(value: &OsStr) -> io::Result<CString> {
    CString::new(value.as_encoded_bytes().to_vec()).map_err(|_| ErrorKind::InvalidInput.into())
}

/// The native wait status of a child process.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ExitStatus(i32);
impl ExitStatus {
    pub fn success(&self) -> bool { self.0 == 0 }
    pub fn code(&self) -> Option<i32> { if self.0 & 0x7f == 0 { Some((self.0 >> 8) & 0xff) } else { None } }
}
impl core::fmt::Debug for ExitStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { write!(f, "ExitStatus(unix_wait_status({}))", self.0) }
}
impl core::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(code) = self.code() { return write!(f, "exit status: {code}"); }
        if self.continued() { return f.write_str("continued (WIFCONTINUED)"); }
        if let Some(signal) = self.signal() {
            write!(f, "signal: {signal}")?;
            if let Some(name) = signal_name(signal) { write!(f, " ({name})")?; }
            if self.core_dumped() { f.write_str(" (core dumped)")?; }
            return Ok(());
        }
        if let Some(signal) = self.stopped_signal() { return write!(f, "stopped (not terminated) by signal: {signal}"); }
        write!(f, "unrecognised wait status: {} {:#x}", self.0, self.0)
    }
}
fn signal_name(signal: i32) -> Option<&'static str> {
    let names = ["", "SIGHUP", "SIGINT", "SIGQUIT", "SIGILL", "SIGTRAP", "SIGABRT", "SIGEMT", "SIGFPE", "SIGKILL", "SIGBUS", "SIGSEGV", "SIGSYS", "SIGPIPE", "SIGALRM", "SIGTERM", "SIGURG", "SIGSTOP", "SIGTSTP", "SIGCONT", "SIGCHLD", "SIGTTIN", "SIGTTOU", "SIGIO", "SIGXCPU", "SIGXFSZ", "SIGVTALRM", "SIGPROF", "SIGWINCH", "SIGINFO", "SIGUSR1", "SIGUSR2"];
    if signal > 0 { names.get(signal as usize).copied() } else { None }
}
mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ExitStatus {}
}
/// Unix operations on the raw status returned by waitpid.
pub trait ExitStatusExt: sealed::Sealed {
    fn from_raw(raw: i32) -> Self;
    fn into_raw(self) -> i32;
    fn signal(&self) -> Option<i32>;
    fn core_dumped(&self) -> bool;
    fn stopped_signal(&self) -> Option<i32>;
    fn continued(&self) -> bool;
}
impl ExitStatusExt for ExitStatus {
    fn from_raw(raw: i32) -> Self { Self(raw) }
    fn into_raw(self) -> i32 { self.0 }
    fn signal(&self) -> Option<i32> { let signal = self.0 & 0x7f; if signal != 0 && signal != 0x7f { Some(signal) } else { None } }
    fn core_dumped(&self) -> bool { self.signal().is_some() && self.0 & 0x80 != 0 }
    fn stopped_signal(&self) -> Option<i32> { if self.0 & 0x7f == 0x7f && !self.continued() { Some(self.0 >> 8) } else { None } }
    fn continued(&self) -> bool { self.0 & 0x7f == 0x7f && self.0 >> 8 == 0x13 }
}

/// The collected output and termination status of a child.
#[derive(Clone, PartialEq, Eq)]
pub struct Output { pub status: ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }
struct DebugBytes<'a>(&'a [u8]);
impl core::fmt::Debug for DebugBytes<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match core::str::from_utf8(self.0) { Ok(text) => core::fmt::Debug::fmt(text, f), Err(_) => core::fmt::Debug::fmt(self.0, f) }
    }
}
impl core::fmt::Debug for Output {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("Output").field("status", &self.status).field("stdout", &DebugBytes(&self.stdout)).field("stderr", &DebugBytes(&self.stderr)).finish() }
}

enum Stream { Inherit, Null, Pipe, File(File) }
/// The child endpoint configuration for a standard stream.
pub struct Stdio { stream: Stream }
impl Stdio {
    pub fn inherit() -> Self { Self { stream: Stream::Inherit } }
    pub fn null() -> Self { Self { stream: Stream::Null } }
    pub fn piped() -> Self { Self { stream: Stream::Pipe } }
}
impl From<File> for Stdio { fn from(file: File) -> Self { Self { stream: Stream::File(file) } } }
pub struct ChildStdin { file: File }
pub struct ChildStdout { file: File }
pub struct ChildStderr { file: File }
impl From<ChildStdin> for Stdio { fn from(pipe: ChildStdin) -> Self { Self::from(pipe.file) } }
impl From<ChildStdout> for Stdio { fn from(pipe: ChildStdout) -> Self { Self::from(pipe.file) } }
impl From<ChildStderr> for Stdio { fn from(pipe: ChildStderr) -> Self { Self::from(pipe.file) } }
impl Write for ChildStdin {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let descriptor = self.file.raw_fd();
        // SAFETY: Darwin's descriptor-local flags suppress SIGPIPE only for
        // this exclusive write. Restore the flag before an endpoint can be
        // transferred to another child's stdout, which must retain SIGPIPE.
        let previous = unsafe { native::fcntl(descriptor, 74) };
        if previous < 0 { return Err(Error::last_os_error()); }
        // SAFETY: descriptor remains live and exclusively owned by this handle.
        if unsafe { native::fcntl(descriptor, 73, 1i32) } < 0 { return Err(Error::last_os_error()); }
        let result = self.file.write(bytes);
        // SAFETY: this restores the exact original descriptor-local policy.
        if unsafe { native::fcntl(descriptor, 73, previous) } < 0 && result.is_ok() { return Err(Error::last_os_error()); }
        result
    }
    fn flush(&mut self) -> io::Result<()> { self.file.flush() }
}
impl Read for ChildStdout { fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> { self.file.read(bytes) } }
impl Read for ChildStderr { fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> { self.file.read(bytes) } }
impl crate::os::unix::io::AsRawFd for ChildStdin { fn as_raw_fd(&self) -> i32 { self.file.raw_fd() } }
impl crate::os::unix::io::AsRawFd for ChildStdout { fn as_raw_fd(&self) -> i32 { self.file.raw_fd() } }
impl crate::os::unix::io::AsRawFd for ChildStderr { fn as_raw_fd(&self) -> i32 { self.file.raw_fd() } }

/// A running child. Dropping the handle closes its pipes but does not wait or kill.
pub struct Child { pub stdin: Option<ChildStdin>, pub stdout: Option<ChildStdout>, pub stderr: Option<ChildStderr>, pid: i32, status: Option<ExitStatus> }
impl Child {
    pub fn id(&self) -> u32 { self.pid as u32 }
    pub fn kill(&mut self) -> io::Result<()> {
        if self.status.is_some() { return Ok(()); }
        // SAFETY: pid is the unreaped child created by this handle; SIGKILL is 9.
        if unsafe { native::kill(self.pid, 9) } == 0 { Ok(()) } else { Err(Error::last_os_error()) }
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> { drop(self.stdin.take()); self.wait_inner(0)?.ok_or_else(|| ErrorKind::Other.into()) }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> { self.wait_inner(1) }
    fn wait_inner(&mut self, options: i32) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status { return Ok(Some(status)); }
        loop {
            let mut raw = 0;
            // SAFETY: this handle owns the specific unreaped child PID and raw
            // supplies initialized writable storage for the native wait status.
            let result = unsafe { native::waitpid(self.pid, &mut raw, options) };
            if result == 0 { return Ok(None); }
            if result > 0 { let status = ExitStatus(raw); self.status = Some(status); return Ok(Some(status)); }
            let error = Error::last_os_error(); if error.kind() != ErrorKind::Interrupted { return Err(error); }
        }
    }
    pub fn wait_with_output(mut self) -> io::Result<Output> {
        drop(self.stdin.take());
        let stdout = self.stdout.take().map(|pipe| pipe.file);
        let stderr = self.stderr.take().map(|pipe| pipe.file);
        match collect_output(stdout, stderr) {
            Ok((stdout, stderr)) => Ok(Output { status: self.wait()?, stdout, stderr }),
            Err(error) => { let _ = self.kill(); let _ = self.wait(); Err(error) }
        }
    }
}

fn collect_output(mut stdout: Option<File>, mut stderr: Option<File>) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::new(); let mut err = Vec::new();
    let mut buffer = [0u8; 8192];
    while stdout.is_some() || stderr.is_some() {
        let mut fds = [native::PollFd { fd: stdout.as_ref().map_or(-1, |file| file.raw_fd()), events: 1, revents: 0 }, native::PollFd { fd: stderr.as_ref().map_or(-1, |file| file.raw_fd()), events: 1, revents: 0 }];
        // SAFETY: fds has exactly two writable native pollfd records, each with
        // a live owned descriptor or -1 (ignored by poll). No timeout is imposed.
        let result = unsafe { native::poll(fds.as_mut_ptr(), 2, -1) };
        if result < 0 { let error = Error::last_os_error(); if error.kind() == ErrorKind::Interrupted { continue; } return Err(error); }
        for index in 0..2 {
            let ready = fds[index].revents;
            if ready & 0x20 != 0 { return Err(Error::from_raw_os_error(9)); }
            if ready & (1 | 8 | 16) == 0 { continue; }
            let (file, bytes) = if index == 0 { (&mut stdout, &mut out) } else { (&mut stderr, &mut err) };
            if let Some(stream) = file.as_mut() {
                match stream.read(&mut buffer) {
                    Ok(0) => { *file = None; }
                    Ok(count) => bytes.extend_from_slice(&buffer[..count]),
                    Err(error) => { if error.kind() != ErrorKind::Interrupted { return Err(error); } }
                }
            }
        }
    }
    Ok((out, err))
}

/// A reusable child-process description. Environment changes never mutate the parent.
pub struct Command {
    program: OsString, arguments: Vec<OsString>, environment: BTreeMap<OsString, Option<OsString>>, clear_environment: bool,
    directory: Option<PathBuf>, stdin: Option<Stdio>, stdout: Option<Stdio>, stderr: Option<Stdio>,
}
impl Command {
    pub fn new<S: AsRef<OsStr>>(program: S) -> Self { Self { program: program.as_ref().to_os_string(), arguments: Vec::new(), environment: BTreeMap::new(), clear_environment: false, directory: None, stdin: None, stdout: None, stderr: None } }
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Self { self.arguments.push(arg.as_ref().to_os_string()); self }
    pub fn args<I, S>(&mut self, args: I) -> &mut Self where I: IntoIterator<Item = S>, S: AsRef<OsStr> { for arg in args { self.arg(arg); } self }
    pub fn env<K: AsRef<OsStr>, V: AsRef<OsStr>>(&mut self, key: K, value: V) -> &mut Self { self.environment.insert(key.as_ref().to_os_string(), Some(value.as_ref().to_os_string())); self }
    pub fn envs<I, K, V>(&mut self, values: I) -> &mut Self where I: IntoIterator<Item = (K, V)>, K: AsRef<OsStr>, V: AsRef<OsStr> { for (key, value) in values { self.env(key, value); } self }
    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Self { self.environment.insert(key.as_ref().to_os_string(), None); self }
    pub fn env_clear(&mut self) -> &mut Self { self.environment.clear(); self.clear_environment = true; self }
    pub fn current_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self { self.directory = Some(path.as_ref().to_path_buf()); self }
    pub fn stdin<T: Into<Stdio>>(&mut self, stream: T) -> &mut Self { self.stdin = Some(stream.into()); self }
    pub fn stdout<T: Into<Stdio>>(&mut self, stream: T) -> &mut Self { self.stdout = Some(stream.into()); self }
    pub fn stderr<T: Into<Stdio>>(&mut self, stream: T) -> &mut Self { self.stderr = Some(stream.into()); self }
    pub fn get_program(&self) -> &OsStr { self.program.as_os_str() }
    pub fn get_current_dir(&self) -> Option<&Path> { self.directory.as_deref() }
    pub fn spawn(&mut self) -> io::Result<Child> { self.spawn_inner(false) }
    pub fn status(&mut self) -> io::Result<ExitStatus> { self.spawn()?.wait() }
    pub fn output(&mut self) -> io::Result<Output> { self.spawn_inner(true)?.wait_with_output() }

    fn resolved_directory(&self) -> io::Result<PathBuf> {
        match &self.directory {
            Some(path) if path.is_absolute() => Ok(path.clone()),
            Some(path) => Ok(crate::env::current_dir()?.join(path)),
            None => crate::env::current_dir(),
        }
    }

    fn spawn_inner(&mut self, capture: bool) -> io::Result<Child> {
        if self.program.as_encoded_bytes().is_empty() { return Err(Error::from_raw_os_error(2)); }
        let mut environment = BTreeMap::new();
        if !self.clear_environment { environment.extend(crate::env::vars_os()); }
        for (key, value) in &self.environment {
            match value { Some(value) => { environment.insert(key.clone(), value.clone()); }, None => { environment.remove(key); } }
        }
        let mut environment_strings = Vec::new();
        for (key, value) in &environment {
            let mut bytes = key.as_encoded_bytes().to_vec(); bytes.push(b'='); bytes.extend_from_slice(value.as_encoded_bytes());
            environment_strings.push(CString::new(bytes).map_err(|_| Error::from(ErrorKind::InvalidInput))?);
        }
        let mut environment_pointers: Vec<*mut c_char> = environment_strings.iter().map(|value| value.as_bytes_with_nul().as_ptr() as *mut c_char).collect(); environment_pointers.push(core::ptr::null_mut());
        let mut argument_strings = Vec::new(); argument_strings.push(c_string(&self.program)?);
        for argument in &self.arguments { argument_strings.push(c_string(argument)?); }
        let mut argument_pointers: Vec<*mut c_char> = argument_strings.iter().map(|value| value.as_bytes_with_nul().as_ptr() as *mut c_char).collect(); argument_pointers.push(core::ptr::null_mut());
        let cwd = self.directory.as_deref().map(|path| c_string(path.as_os_str())).transpose()?;
        let (input, parent_input) = prepare_stream(self.stdin.as_ref(), capture, false)?;
        let (output, parent_output) = prepare_stream(self.stdout.as_ref(), capture, true)?;
        let (error, parent_error) = prepare_stream(self.stderr.as_ref(), capture, true)?;
        let mut actions = Actions::new()?;
        for (target, file) in [(0, &input), (1, &output), (2, &error)] {
            if let Some(file) = file { actions.duplicate(file.raw_fd(), target)?; }
            else {
                // SAFETY: F_GETFD queries only this process's descriptor table.
                if unsafe { native::fcntl(target, 1) } >= 0 { actions.inherit(target)?; }
            }
        }
        if let Some(cwd) = &cwd { actions.chdir(cwd)?; }
        let attributes = Attributes::new()?;
        let mut paths = Vec::new();
        let search_path = !self.program.as_encoded_bytes().contains(&b'/');
        if !search_path {
            let path = Path::new(&self.program);
            paths.push((if path.is_absolute() { path.to_path_buf() } else { self.resolved_directory()?.join(path) }, path.to_path_buf()));
        } else {
            let search = environment.get(OsStr::new("PATH")).cloned().unwrap_or_else(|| OsString::from("/usr/bin:/bin"));
            for path in search.as_encoded_bytes().split(|&byte| byte == b':') {
                let path = Path::new(OsStr::from_unix_bytes(path));
                let script = path.join(Path::new(&self.program));
                let executable = if script.is_absolute() { script.clone() } else { self.resolved_directory()?.join(&script) };
                paths.push((executable, script));
            }
        }
        let mut denied = false;
        for (path, script) in paths {
            let program = c_string(path.as_os_str())?;
            let mut pid = 0;
            // SAFETY: all opaque state was initialized by libc, each pointer
            // list is NUL-terminated, and all strings/FD owners outlive spawn.
            let mut code = unsafe { native::posix_spawn(&mut pid, program.as_bytes_with_nul().as_ptr() as *const c_char, &actions.0, &attributes.0, argument_pointers.as_ptr(), environment_pointers.as_ptr()) };
            if code == 8 {
                // execvp's ENOEXEC fallback runs a readable script through sh.
                // Preserve all arguments after argv[0], including the trailing NUL.
                let script = c_string(script.as_os_str())?;
                let mut shell_arguments = Vec::new();
                shell_arguments.push(c"/bin/sh".as_ptr() as *mut c_char);
                shell_arguments.push(script.as_bytes_with_nul().as_ptr() as *mut c_char);
                shell_arguments.extend_from_slice(&argument_pointers[1..]);
                // SAFETY: both pointer arrays remain terminated and all backing
                // strings/action objects stay live for this synchronous spawn.
                code = unsafe { native::posix_spawn(&mut pid, c"/bin/sh".as_ptr(), &actions.0, &attributes.0, shell_arguments.as_ptr(), environment_pointers.as_ptr()) };
                if code != 0 { return Err(Error::from_raw_os_error(code)); }
            }
            if code == 0 { return Ok(Child { stdin: parent_input.map(|file| ChildStdin { file }), stdout: parent_output.map(|file| ChildStdout { file }), stderr: parent_error.map(|file| ChildStderr { file }), pid, status: None }); }
            if !search_path { return Err(Error::from_raw_os_error(code)); }
            if code == 13 { denied = true; } else if code != 2 && code != 20 { return Err(Error::from_raw_os_error(code)); }
        }
        Err(Error::from_raw_os_error(if denied { 13 } else { 2 }))
    }
}

fn normalize_file(file: File) -> io::Result<File> {
    // SAFETY: F_DUPFD_CLOEXEC atomically creates a distinct owned descriptor
    // above the standard streams, with close-on-exec set from its creation.
    let descriptor = unsafe { native::fcntl(file.raw_fd(), 67, 3i32) };
    if descriptor < 0 { Err(Error::last_os_error()) } else {
        // SAFETY: successful fcntl transferred a new unique descriptor to us.
        Ok(unsafe { File::from_raw_fd_unchecked(descriptor) })
    }
}
fn pipe_pair() -> io::Result<(File, File)> {
    let mut descriptors = [-1i32; 2];
    // SAFETY: pipe writes two owned descriptors into the supplied storage.
    if unsafe { native::pipe(descriptors.as_mut_ptr()) } != 0 { return Err(Error::last_os_error()); }
    // SAFETY: pipe succeeded and returned two distinct owned descriptors.
    let read = unsafe { File::from_raw_fd_unchecked(descriptors[0]) };
    // SAFETY: the second descriptor is separately owned and has not been wrapped.
    let write = unsafe { File::from_raw_fd_unchecked(descriptors[1]) };
    Ok((normalize_file(read)?, normalize_file(write)?))
}
fn prepare_stream(stream: Option<&Stdio>, capture: bool, output: bool) -> io::Result<(Option<File>, Option<File>)> {
    let default = if capture { if output { Stdio::piped() } else { Stdio::null() } } else { Stdio::inherit() };
    match &stream.unwrap_or(&default).stream {
        Stream::Inherit => Ok((None, None)),
        Stream::Null => Ok((Some(normalize_file(File::options().read(!output).write(output).open("/dev/null")?)?), None)),
        Stream::File(file) => Ok((Some(normalize_file(file.try_clone()?)?), None)),
        Stream::Pipe => {
            let (read, write) = pipe_pair()?;
            if output { Ok((Some(write), Some(read))) } else {
                Ok((Some(read), Some(write)))
            }
        }
    }
}
struct Actions(*mut c_void);
impl Actions {
    fn new() -> io::Result<Self> {
        let mut value = core::ptr::null_mut();
        // SAFETY: libc initializes an opaque file-action object in this slot.
        unsafe { error_code(native::posix_spawn_file_actions_init(&mut value))?; }
        Ok(Self(value))
    }
    fn duplicate(&mut self, from: i32, to: i32) -> io::Result<()> {
        // SAFETY: this action object is initialized and from is an owned live FD.
        unsafe { error_code(native::posix_spawn_file_actions_adddup2(&mut self.0, from, to)) }
    }
    fn inherit(&mut self, descriptor: i32) -> io::Result<()> {
        // SAFETY: the caller checked that this inherited standard FD is open.
        unsafe { error_code(native::posix_spawn_file_actions_addinherit_np(&mut self.0, descriptor)) }
    }
    fn chdir(&mut self, directory: &CString) -> io::Result<()> {
        // SAFETY: the action object is initialized and directory retains its NUL.
        unsafe { error_code(native::posix_spawn_file_actions_addchdir_np(&mut self.0, directory.as_bytes_with_nul().as_ptr() as *const c_char)) }
    }
}
impl Drop for Actions {
    fn drop(&mut self) {
        // SAFETY: this unique guard destroys its successfully initialized object.
        unsafe { native::posix_spawn_file_actions_destroy(&mut self.0); }
    }
}
struct Attributes(*mut c_void);
impl Attributes {
    fn new() -> io::Result<Self> {
        let mut value = core::ptr::null_mut();
        // SAFETY: libc initializes an opaque spawn-attribute object in this slot.
        unsafe { error_code(native::posix_spawnattr_init(&mut value))?; }
        let mut result = Self(value);
        // SAFETY: the opaque object is live. Darwin sigset_t is a u32 bitset;
        // bit 12 resets SIGPIPE(13) in the child. CLOEXEC_DEFAULT closes all
        // descriptors not explicitly retained by the file actions.
        unsafe {
            let signals = 1u32 << 12;
            error_code(native::posix_spawnattr_setsigdefault(&mut result.0, &signals))?;
            error_code(native::posix_spawnattr_setflags(&mut result.0, 0x4000 | 0x0004))?;
        }
        Ok(result)
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: this unique guard destroys its successfully initialized object.
        unsafe { native::posix_spawnattr_destroy(&mut self.0); }
    }
}

impl core::fmt::Debug for ChildStdin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("ChildStdin").finish_non_exhaustive() }
}
impl core::fmt::Debug for ChildStdout {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("ChildStdout").finish_non_exhaustive() }
}
impl core::fmt::Debug for ChildStderr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("ChildStderr").finish_non_exhaustive() }
}
impl core::fmt::Debug for Child {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("Child").field("stdin", &self.stdin).field("stdout", &self.stdout).field("stderr", &self.stderr).finish_non_exhaustive() }
}
impl core::fmt::Debug for Stdio {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("Stdio").finish_non_exhaustive() }
}
