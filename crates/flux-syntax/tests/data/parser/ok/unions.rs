/// A union declaration, with shared record-field syntax.
#[derive(Copy, Clone)]
#[repr(C)]
pub union Bits {
    pub byte: u8,
    word: u64,
    #[cfg(target_arch = "aarch64")]
    neon: [u64; 2],
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    sse2: [u8; 16],
}

union Generic<T: Copy> where T: Sized {
    value: T,
    empty: (),
}

mod names {
    union union {
        union: u32,
        r#type: u64,
    }
}

fn union(union: u32) -> u32 {
    let r#union = union + 1;
    r#union
}

macro_rules! union { () => { 1 }; }
fn main() {
    union Local { value: u32 }
    let union = union!( );
    let _ = union;
}
