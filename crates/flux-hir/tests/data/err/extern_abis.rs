extern "stdcall" fn stdcall() {}
extern "cdecl" fn cdecl() {}
extern "vectorcall" fn vectorcall() {}
type Bogus = extern "bogus" fn();
unsafe extern "Rust" {
    fn imported();
}
unsafe extern "fastcall-unwind" {
    fn fastcall();
}
#[track_caller]
extern "C-unwind" fn tracked() {}
extern "system" fn main() {}
