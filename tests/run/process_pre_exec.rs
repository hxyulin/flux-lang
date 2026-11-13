use std::os::unix::prelude::*;
use std::process::Command;
unsafe extern "C" { fn write(fd:i32,bytes:*const u8,len:usize)->isize; }
fn main() {
    let mut command=Command::new("/bin/sh"); command.args(["-c","printf executed"]);
    // SAFETY: both hooks only write fixed bytes through libc and return.
    unsafe {
        command.pre_exec(|| { write(1,b"before ".as_ptr(),7); Ok(()) });
        command.pre_exec(|| { write(1,b"after ".as_ptr(),6); Ok(()) });
    }
    for _ in 0..2 {
        let output=command.output().unwrap(); assert!(output.status.success());
        assert_eq!(output.stdout,b"before after executed");
    }
    for code in [0, 13] {
        let mut failed=Command::new("/bin/sh");
        // SAFETY: constructing this OS error requires no allocation or locks.
        unsafe { failed.pre_exec(move || Err(std::io::Error::from_raw_os_error(code))); }
        let error=match failed.spawn() {Ok(_)=>panic!("callback must fail"),Err(e)=>e};
        assert_eq!(error.raw_os_error(),Some(code));
    }
    let mut missing=Command::new("/flux-does-not-exist");
    // SAFETY: the no-op callback performs no operations.
    unsafe {missing.pre_exec(||Ok(()));}
    let error=match missing.spawn() {Ok(_)=>panic!("exec must fail"),Err(e)=>e};
    assert_eq!(error.raw_os_error(),Some(2));
    println!("callbacks ordered, reusable, and errors returned");
}
