//! Strings sliced by ranges of bytes at character boundaries, and the
//! methods that give parts of them: `split_at`, `trim`, `split` (by a
//! `char`, a `&str` and the empty string), `lines`, `split_whitespace`;
//! and `char::encode_utf8`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(s: &str) {
    unsafe { printf(c"[%.*s]".as_ptr(), s.len() as i32, s.as_ptr()) };
}

fn main() {
    let s = "  héllo, wörld  \n";
    show(&s[2..8]);
    show(&s[..5]);
    show(&s[9..]);
    show(&s[..]);
    show(&s[2..=4]);
    show(s.trim());
    show(s.trim_start());
    show(s.trim_end());
    let (a, b) = s.split_at(5);
    show(a);
    show(b);
    unsafe { printf(c"\n".as_ptr()) };
    for part in "a,b,,c,".split(',') {
        show(part);
    }
    unsafe { printf(c"\n".as_ptr()) };
    for part in "one::two::three".split("::") {
        show(part);
    }
    for part in "abc".split("") {
        show(part);
    }
    for part in "".split(',') {
        show(part);
    }
    unsafe { printf(c"\n".as_ptr()) };
    for line in "first\nsecond\r\n\nfourth\r\nfifth\r".lines() {
        show(line);
    }
    unsafe { printf(c"\n".as_ptr()) };
    for w in "  the quick\tbrown\n fox ".split_whitespace() {
        show(w);
    }
    unsafe { printf(c"\n".as_ptr()) };
    let mut buf = [0u8; 4];
    let e = 'é'.encode_utf8(&mut buf);
    show(e);
    let n = e.len();
    unsafe { printf(c" %zu %d %d\n".as_ptr(), n, buf[0] as i32, buf[1] as i32) };
    let words = "x y z".split(' ').count();
    unsafe { printf(c"%zu\n".as_ptr(), words) };
}
