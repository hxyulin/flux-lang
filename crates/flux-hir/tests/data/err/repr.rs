// `#[repr]` on enums: the discriminant's type bounds the values, and
// hints are reported where malformed, conflicting or unsupported.

#[repr(u8)]
enum Overflowed {
    A = 255,
    B,
}

#[repr(u8)]
enum TooBig {
    A = 256,
}

#[repr(i8)]
enum TooSmall {
    A = -129,
}

#[repr(u8)]
enum Negative {
    A = -1,
}

#[repr(u8)]
enum Suffixed {
    A = 1u8,
    B = 2i32,
}

#[repr(u8, u16)]
enum TwoInts {
    A,
}

#[repr(u8)]
#[repr(u16)]
enum TwoAttributes {
    A,
}

#[repr(C, u8)]
enum CAndIntWithoutFields {
    A,
}

#[repr(C)]
enum COnlyWithFields {
    A = 1,
    B(i32),
}

#[repr(u8)]
enum Empty {}

#[repr]
enum NoHints {
    A,
}

#[repr(foo)]
enum Unknown {
    A,
}

#[repr(u8 = "x")]
enum Assigned {
    A,
}

#[repr(transparent)]
enum Transparent {
    A,
}

fn main() {}
