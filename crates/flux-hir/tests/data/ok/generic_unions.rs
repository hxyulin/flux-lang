// Generic unions preserve their parameters and cfg-filtered fields.
#[derive(Copy, Clone)]
#[repr(C)]
union Bits<T: Copy> {
    byte: T,
    word: u64,
    #[cfg(all())]
    enabled: [u64; 2],
    #[cfg(any())]
    disabled: [u8; 16],
}

fn main() {}
