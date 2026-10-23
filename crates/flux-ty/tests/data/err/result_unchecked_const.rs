const OK: u32 = unsafe { Result::<u32, ()>::Ok(17).unwrap_unchecked() };
const ERR: u32 = unsafe { Result::<(), u32>::Err(23).unwrap_err_unchecked() };

const fn take_ok(value: Result<u32, ()>) -> u32 {
    // SAFETY: callers must pass Ok.
    unsafe { value.unwrap_unchecked() }
}

const fn take_err(value: Result<(), u32>) -> u32 {
    // SAFETY: callers must pass Err.
    unsafe { value.unwrap_err_unchecked() }
}

fn main() {}
