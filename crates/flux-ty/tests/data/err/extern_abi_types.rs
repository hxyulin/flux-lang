trait Marker {}
impl Marker for extern "C" fn() {}
impl Marker for extern "C-unwind" fn() {}
impl Marker for extern "system" fn() {}
impl Marker for extern "system-unwind" fn() {}
impl Marker for extern "C-unwind" fn() {}
extern "system" fn system() {}
extern "C-unwind" fn unwind() {}
fn main() {
    let a: extern "C" fn() = system;
    let b: extern "system" fn() = unwind;
    let c: extern "C-unwind" fn() = unwind;
    let d: extern "C" fn() = c;
    let e: unsafe extern "C-unwind" fn() = c;
    let f: fn() = || {};
    let g: extern "system" fn() = || {};
}
