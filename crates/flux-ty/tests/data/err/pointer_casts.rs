fn casts(x: &i32, s: &[u8], fp: *const [u8], p: *const i32, b: bool, c: char, m: &mut i32) {
    let _ = x as usize;
    let _ = s as *const u8;
    let _ = fp as usize;
    let _ = c as *const u8;
    let _ = b as *const u8;
    let _ = x as *mut i32;
    let _ = p as *const [u8];
    let _ = p as bool;
    let _ = x as *const u8;
    let _ = m as *mut i32;
    let _ = fp as *const u8;
    let _ = p as u8;
    let _ = 5u16 as *const u8;
}

fn coercions(x: &i32, m: &mut i32) {
    let a: *const i32 = x;
    let b: *mut i32 = x;
    let c: *const u8 = x;
    let d: *const i32 = m;
    let e: *mut i32 = m;
}

fn main() {}
