//! Main return values and process exit status codes.

/// The status returned to the operating system when a program finishes.
#[derive(Clone, Copy, PartialEq)]
pub struct ExitCode(u8);

impl ExitCode {
    /// The canonical successful status.
    pub const SUCCESS: Self = Self(0);
    /// The canonical unsuccessful status.
    pub const FAILURE: Self = Self(1);
}
impl Default for ExitCode {
    fn default() -> Self { Self::SUCCESS }
}
impl From<u8> for ExitCode {
    fn from(code: u8) -> Self { Self(code) }
}
impl core::fmt::Debug for ExitCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ExitCode(unix_exit_status({}))", self.0)
    }
}

/// Converts a program's return value into its process exit status.
pub trait Termination {
    /// Reports the outcome of the program.
    fn report(self) -> ExitCode;
}
impl Termination for () {
    fn report(self) -> ExitCode { ExitCode::SUCCESS }
}
impl Termination for ExitCode {
    fn report(self) -> ExitCode { self }
}
impl Termination for core::convert::Infallible {
    fn report(self) -> ExitCode { match self {} }
}
impl<T: Termination, E: core::fmt::Debug> Termination for Result<T, E> {
    fn report(self) -> ExitCode {
        match self {
            Ok(value) => value.report(),
            Err(error) => {
                eprintln!("Error: {error:?}");
                ExitCode::FAILURE
            }
        }
    }
}

/// Entry adapter instantiated by the compiler for non-unit main return values.
#[doc(hidden)]
pub fn __flux_run_main<T: Termination>(main: fn() -> T) -> i32 {
    main().report().0 as i32
}

unsafe extern "C" {
    #[link_name = "exit"]
    fn native_exit(code: i32) -> !;
}

/// Terminates the process without unwinding Rust stack frames.
/// Buffered output is flushed before native process-exit callbacks run.
pub fn exit(code: i32) -> ! {
    core::io::flush_stdout();
    // SAFETY: libc exit accepts every status and terminates this process.
    // Normal native exit callbacks, including Darwin TLS cleanup, stay active.
    unsafe { native_exit(code) }
}
