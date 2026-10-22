// Generic unions remain outside the executable scalar subset.
#[derive(Copy, Clone)]
#[repr(C)]
union Bits<T: Copy> {
    byte: T,
    word: u64,
    #[cfg(target_arch = "aarch64")]
    neon: [u64; 2],
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    sse2: [u8; 16],
}

fn main() {}
