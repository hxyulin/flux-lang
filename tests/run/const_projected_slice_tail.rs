trait Encode {
    type Bytes;
}
struct CodePoint;
#[repr(transparent)]
struct U24([u8; 3]);
impl Encode for CodePoint {
    type Bytes = U24;
}
#[repr(transparent)]
struct Encoded<T: Encode>([T::Bytes]);

impl<T: Encode> Encoded<T> {
    pub const fn from_bytes(slice: &[T::Bytes]) -> &Self {
        unsafe { &*(slice as *const _ as *const Self) }
    }
}

struct Borrowed<'a, T: Encode>(&'a [T::Bytes]);
impl<T: Encode> Encoded<T> {
    const fn borrowed(&self) -> Borrowed<'_, T> {
        Borrowed(&self.0)
    }
}
const POINTS: Borrowed<'static, CodePoint> =
    Encoded::from_bytes(const { &[U24([0, 0, 0]), U24([0, 0, 1])] }).borrowed();

fn main() {
    assert_eq!(POINTS.0[1].0[2], 1);
}
