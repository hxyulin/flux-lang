//! MISSING FEATURE (documented). A tuple variant's constructor used as a
//! function value: `opt.map_or(Atom::Byte(c), Atom::Class)`.
//!
//! rustc: compiles; exits with status 7.
//! flux: "tuple variant constructors as values are not supported yet".
//! Blocks round2/matchers.rs.

enum Atom {
    Byte(u8),
    Class(u32),
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let atom = Some(7).map_or(Atom::Byte(1), Atom::Class);
    let code = match atom {
        Atom::Byte(b) => b as i32,
        Atom::Class(c) => c as i32,
    };
    unsafe { exit(code) }
}
