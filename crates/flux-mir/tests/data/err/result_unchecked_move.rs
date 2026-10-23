struct Payload(i32);

fn main() {
    let ok: Result<Payload, ()> = Ok(Payload(17));
    // SAFETY: ok is Ok.
    let value = unsafe { ok.unwrap_unchecked() };
    let _again = ok;
    let err: Result<(), Payload> = Err(Payload(23));
    // SAFETY: err is Err.
    let error = unsafe { err.unwrap_err_unchecked() };
    let _again = err;
}
