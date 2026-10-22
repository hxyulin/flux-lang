//! `core`'s functions named as values, never called directly: passed to
//! adapters as function items, generic ones included, and coerced to
//! function pointers. Each must be compiled though only a shim or a
//! pointer reaches it.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn parse(b: u8) -> Result<u8, u8> {
    if b.is_ascii_digit() { Ok(b - b'0') } else { Err(b) }
}

fn call_ptr(f: fn(&u8) -> bool, b: u8) -> bool {
    f(&b)
}

fn hold<F: Fn(&u8) -> bool>(f: F) -> (F, i32) {
    (f, 1)
}

fn main() {
    let text: &[u8] = b"1f2a";
    show(1, text.iter().all(u8::is_ascii_hexdigit) as i32);
    show(2, b"12z4".iter().all(u8::is_ascii_hexdigit) as i32);
    let ok = b"1a2b3".iter().map(|&b| parse(b)).filter(Result::is_ok).count();
    show(3, ok as i32);
    let none = [Some(1), None, Some(3)].iter().filter(|o| o.is_none()).count();
    show(4, none as i32);
    let p: fn(&u8) -> bool = u8::is_ascii_uppercase;
    show(5, p(&b'Q') as i32 + call_ptr(u8::is_ascii_lowercase, b'q') as i32);
    let (f, n) = hold(u8::is_ascii_digit);
    show(6, f(&b'7') as i32 + n);
    let firsts: i32 = [Some(4), None, Some(5)].into_iter().map(Option::unwrap_or_default).sum();
    show(7, firsts);
}
