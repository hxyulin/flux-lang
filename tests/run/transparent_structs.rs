use std::marker::PhantomData;
use std::mem::{align_of, size_of};

#[repr(transparent)]
struct Bytes<E>([u8; 4], PhantomData<E>);
#[repr(transparent)]
struct Ref<'a>((), &'a u32, PhantomData<&'a u32>);
#[repr(transparent)]
struct Wrap<T: ?Sized>(PhantomData<()>, T);
#[repr(align(32))]
struct Aligned;
#[repr(transparent)]
struct AlignedWrap((), Aligned);
#[repr(transparent)]
struct NonZero(PhantomData<()>, core::num::NonZeroU64, ());
#[repr(transparent)]
struct Empty;
#[repr(transparent)]
struct Units((), ());
const REF: Option<Ref<'static>> = Some(Ref((), &VALUE, PhantomData));
static VALUE: u32 = 37;
const BYTES: Bytes<u64> = Bytes([1, 2, 3, 4], PhantomData);

fn main() {
    assert_eq!(size_of::<Bytes<u64>>(), 4);
    assert_eq!(align_of::<Bytes<u64>>(), 1);
    assert_eq!(BYTES.0, [1, 2, 3, 4]);
    assert_eq!(size_of::<Ref<'_>>(), size_of::<&u32>());
    assert_eq!(size_of::<Option<Ref<'_>>>(), size_of::<&u32>());
    assert_eq!(*REF.unwrap().1, VALUE);
    let pointer = Ref((), &VALUE, PhantomData);
    let bits: usize = unsafe { std::mem::transmute(pointer) };
    assert_eq!(bits, &VALUE as *const u32 as usize);
    let none: usize = unsafe { std::mem::transmute(None::<Ref<'_>>) };
    assert_eq!(none, 0);
    assert_eq!(size_of::<Option<NonZero>>(), 8);
    let nonzero = NonZero(PhantomData, core::num::NonZeroU64::new(19).unwrap(), ());
    assert_eq!(nonzero.1.get(), 19);
    assert_eq!(size_of::<Empty>(), 0);
    assert_eq!(size_of::<Units>(), 0);
    assert_eq!(align_of::<Units>(), 1);
    assert_eq!(size_of::<AlignedWrap>(), 0);
    assert_eq!(align_of::<AlignedWrap>(), 32);
    let wrapper = Wrap(PhantomData, [5u8, 6, 7]);
    let slice: &Wrap<[u8]> = &wrapper;
    assert_eq!(&slice.1, &[5, 6, 7]);
    assert_eq!(std::mem::size_of_val(slice), 3);
    let owned = Wrap(PhantomData, String::from("transparent"));
    assert_eq!(owned.1.len(), 11);
    println!(
        "transparent {} {} {} {}",
        BYTES.0[2],
        *REF.unwrap().1,
        slice.1.len(),
        owned.1
    );
}
