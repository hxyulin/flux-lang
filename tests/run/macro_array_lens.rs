//! Array lengths written as expressions (anonymous constants) in items a
//! `macro_rules!` macro defines: in signatures, associated types, field
//! types and repeat counts in bodies.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

fn len_of<const N: usize>(_: [u8; N]) -> usize {
    N
}

trait Digest {
    type Output;
    fn digest(&self) -> Self::Output;
}

macro_rules! bytes_of {
    ($name:ident, $ty:ty) => {
        struct $name {
            value: $ty,
            spare: [u8; 1 + 1],
        }

        impl Digest for $name {
            type Output = [u8; (<$ty>::BITS / 8) as usize];
            fn digest(&self) -> Self::Output {
                let mut out = [0u8; (<$ty>::BITS / 8) as usize];
                let mut v = self.value;
                for b in out.iter_mut() {
                    *b = (v & 0xff) as u8;
                    v >>= 8;
                }
                out
            }
        }
    };
}

bytes_of!(Word, u32);
bytes_of!(Half, u16);

macro_rules! make {
    () => {
        fn make() -> [u8; 2 + 2] {
            [7; 2 * 2]
        }
    };
}

make!();

fn main() {
    show(1, len_of(make()) as i32);
    show(2, make().iter().map(|&b| b as i32).sum());
    let w = Word {
        value: 0x0403_0201,
        spare: [0; 2],
    };
    let d = w.digest();
    show(3, (len_of(d) * 1000) as i32 + d[3] as i32 * 100 + d[0] as i32 + w.spare.len() as i32);
    let h = Half {
        value: 0x0905,
        spare: [1; 2],
    };
    show(4, len_of(h.digest()) as i32 * 100 + h.digest()[1] as i32);
}
