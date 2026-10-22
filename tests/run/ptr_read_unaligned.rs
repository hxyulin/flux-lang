// Each width at every byte offset, including a read ending at the backing
// allocation's boundary. Expected values are assembled independently.
fn exercise(bytes: &[u8]) {
    let mut offset = 0;
    while offset < bytes.len() {
        let p = unsafe { bytes.as_ptr().add(offset) };
        let mut expected = 0u64;
        let mut width = 1;
        while width <= 8 && offset + width <= bytes.len() {
            expected |= (bytes[offset + width - 1] as u64) << ((width - 1) * 8);
            unsafe {
                if width == 1 {
                    assert_eq!(p.read_unaligned() as u64, expected);
                    assert_eq!(core::ptr::read_unaligned(p) as u64, expected);
                    assert_eq!((p as *mut u8).read_unaligned() as u64, expected);
                } else if width == 2 {
                    assert_eq!(p.cast::<u16>().read_unaligned() as u64, expected);
                    assert_eq!(core::ptr::read_unaligned(p.cast::<u16>()) as u64, expected);
                    assert_eq!((p as *mut u16).read_unaligned() as u64, expected);
                } else if width == 4 {
                    assert_eq!(p.cast::<u32>().read_unaligned() as u64, expected);
                    assert_eq!(core::ptr::read_unaligned(p.cast::<u32>()) as u64, expected);
                    assert_eq!((p as *mut u32).read_unaligned() as u64, expected);
                } else if width == 8 {
                    assert_eq!(p.cast::<u64>().read_unaligned(), expected);
                    assert_eq!(core::ptr::read_unaligned(p.cast::<u64>()), expected);
                    assert_eq!((p as *mut u64).read_unaligned(), expected);
                    assert_eq!(p.cast::<usize>().read_unaligned() as u64, expected);
                    assert_eq!(core::ptr::read_unaligned(p.cast::<usize>()) as u64, expected);
                    assert_eq!((p as *mut usize).read_unaligned() as u64, expected);
                }
            }
            width += 1;
        }
        offset += 1;
    }
    println!("checked {} offsets", bytes.len());
}

fn main() {
    // Nonuniform bytes, including high bits, reveal endian and extension bugs.
    let bytes = [0x81u8, 2, 0xff, 4, 0x80, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0xfe];
    exercise(&bytes);
    exercise(&bytes[1..]);
    exercise(&[0xff]);
}
