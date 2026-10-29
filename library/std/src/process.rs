//! Process identity and program termination on the supported Unix hosts.
mod termination;
pub use self::termination::{ExitCode, Termination, exit};
unsafe extern "C" {fn getpid()->i32;}
/// Returns this process's operating-system identifier.
pub fn id()->u32 {
    // SAFETY: getpid takes no pointers and is available on supported Unix hosts.
    unsafe {getpid() as u32}
}

#[cfg(target_os = "macos")]
pub(crate) mod command;
#[cfg(target_os = "macos")]
pub use self::command::{Child, ChildStdin, ChildStdout, ChildStderr, Command, Output, ExitStatus, Stdio};
