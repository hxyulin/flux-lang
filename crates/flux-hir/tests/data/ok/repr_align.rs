#[repr(C, align(0x40,))]
struct Aligned<T>(T);
#[repr(align(8))]
#[repr(align(32))]
#[repr(align(16))]
struct Maximum;
#[repr(align(536_870_912))]
struct Limit;
#[repr(u8, align(64))]
enum Tagged { A(u8), B }

fn main() {}
