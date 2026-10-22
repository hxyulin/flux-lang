//! WRONG REJECTION. A closure that captures a `&'a str` parameter by
//! reference and copies it into its result, passed to `Result::map_err`,
//! whose result is returned.
//!
//! rustc: compiles; exits with status 47.
//! flux: "error: closure may outlive the current function, but it borrows
//! `key`, which is owned by the current function" (at the closure).
//! flux assumes a generic call's result may hold the closure's borrows
//! (of `key` itself, a local), though the result's type
//! (`Result<i32, Error<'a>>`) can only hold `'a` data; rustc relates them
//! through the signature only. Calling the closure directly
//! (`let f = || key; f()`) is accepted. Found in round2/ini_csv.rs.

#[derive(Clone, Copy)]
enum Error<'a> {
    Bad { key: &'a str, code: i32 },
}

fn check<'a>(key: &'a str, r: Result<i32, i32>) -> Result<i32, Error<'a>> {
    r.map_err(|code| Error::Bad { key, code })
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let code = match check("port", Err(7)) {
        Ok(v) => v,
        Err(Error::Bad { key, code }) => key.len() as i32 * 10 + code,
    };
    unsafe { exit(code) }
}
