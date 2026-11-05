//! Process identity and program termination on the supported Unix hosts.
mod termination;
pub use self::termination::{ExitCode, Termination, exit};
unsafe extern "C" {fn getpid()->i32;}
/// Returns this process's operating-system identifier.
pub fn id()->u32 {
    // SAFETY: getpid takes no pointers and is available on supported Unix hosts.
    unsafe {getpid() as u32}
}

#[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) mod command;
#[cfg(any(target_os = "macos", all(target_os = "linux", target_arch = "x86_64")))]
pub use self::command::{Child, ChildStdin, ChildStdout, ChildStderr, Command, CommandArgs, CommandEnvs, Output, ExitStatus, Stdio};

/// Terminates the process abnormally with SIGABRT, without running
/// destructors or flushing buffered output.
pub fn abort() -> ! {
    core::panicking::abort_now()
}
