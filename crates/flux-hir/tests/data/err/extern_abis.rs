extern "stdcall" fn stdcall() {}
extern "cdecl" fn cdecl() {}
extern "vectorcall" fn vectorcall() {}
type Bogus = extern "bogus" fn();
unsafe extern "fastcall-unwind" {
    fn fastcall();
}
#[track_caller]
extern "C-unwind" fn tracked() {}
extern "system" fn main() {}
unsafe extern "Rust" {
    fn invalid_rust_variadic(value: i32, ...);
}
