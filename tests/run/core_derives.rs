//! Derives inside `core` (`Utf8Error`'s `Clone`, `Copy`, `PartialEq`,
//! `Debug`), `Default` for `char` and `&str` through a derive, and
//! `Result::unwrap_err`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

#[derive(Debug, Default)]
struct Named<'a> {
    name: &'a str,
    initial: char,
    rest: &'a str,
}

fn main() {
    let bad: &[u8] = &[104, 0xff, 105];
    let e = core::str::from_utf8(bad).unwrap_err();
    let e2 = e;
    let n = Named::default();
    let c: char = Default::default();
    unsafe {
        printf(
            c"%d %d %d %d %d\n".as_ptr(),
            e.valid_up_to() as i32,
            (e == e2) as i32,
            n.name.len() as i32 + n.rest.len() as i32,
            n.initial as i32,
            c as i32,
        );
    }
}
