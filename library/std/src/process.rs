//! Process identity on the supported Unix hosts.
unsafe extern "C" {fn getpid()->i32;}
/// Returns this process's operating-system identifier.
pub fn id()->u32 {
    // SAFETY: getpid takes no pointers and is available on supported Unix hosts.
    unsafe {getpid() as u32}
}
