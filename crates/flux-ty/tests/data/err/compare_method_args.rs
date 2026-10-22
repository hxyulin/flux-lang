// A comparison method's argument is coerced to the parameter the only
// `impl` block of `str` or `[T]` gives it (E0308 where it cannot be),
// and `&&&str` derefs to `&str`.

fn main() {
    let a = "abc";
    let x: &[i32] = &[1, 2];
    let _ = a.cmp(&1);
    let _ = x.partial_cmp("a");
    let _ = a.partial_cmp(&&&a);
}
