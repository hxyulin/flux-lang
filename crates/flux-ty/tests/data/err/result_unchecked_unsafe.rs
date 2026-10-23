fn main() {
    let ok: Result<u32, ()> = Ok(17);
    ok.unwrap_unchecked();
    let err: Result<(), u32> = Err(23);
    err.unwrap_err_unchecked();
}
