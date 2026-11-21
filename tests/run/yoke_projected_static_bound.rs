trait Family<'a> {
    type Slice: ?Sized;
}
struct Borrowed<'a, K: Family<'a> + ?Sized>(&'a K::Slice);

trait Yoke<'a>: 'static {
    type Output;
    fn transform_owned(self) -> Self::Output;
}

impl<'a, K: ?Sized + 'static + for<'b> Family<'b>> Yoke<'a> for Borrowed<'static, K>
where
    &'static <K as Family<'static>>::Slice: for<'b> Yoke<'b>,
{
    type Output = Borrowed<'a, K>;
    fn transform_owned(self) -> Self::Output {
        let this = core::mem::ManuallyDrop::new(self);
        let ptr: *const Self::Output = (&*this as *const Self).cast();
        unsafe { core::ptr::read(ptr) }
    }
}

struct Key;
impl Family<'_> for Key {
    type Slice = [u32];
}
impl<'a> Yoke<'a> for &'static [u32] {
    type Output = &'a [u32];
    fn transform_owned(self) -> Self::Output {
        self
    }
}

fn main() {
    static DATA: [u32; 3] = [11, 13, 17];
    let transformed = Borrowed::<Key>(&DATA).transform_owned();
    assert_eq!(transformed.0[1], 13);
}
