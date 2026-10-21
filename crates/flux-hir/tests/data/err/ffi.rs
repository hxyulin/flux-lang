unsafe extern "C" {
    fn first(..., x: i32);
    fn named(x: i32, rest: ...);
}

fn not_foreign(x: i32, ...) {}

fn literals() {
    let a = c"nul \0 inside";
    let b = c"escaped \x00 nul";
    let c = c"unknown \q escape";
    let d = b"bytes";
}
