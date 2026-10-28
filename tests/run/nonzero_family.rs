use core::num::*;
macro_rules! check {
    ($name:ident,$int:ty,$value:expr) => {{
        const VALUE: $name = $name::new($value).unwrap();
        assert_eq!(VALUE.get(), $value);
        assert!($name::new(0).is_none());
        assert!($name::try_from(0).is_err());
        assert_eq!(<$int>::from(VALUE), $value);
        assert_eq!(core::mem::size_of::<Option<$name>>(), core::mem::size_of::<$int>());
        assert_eq!(core::mem::align_of::<Option<$name>>(), core::mem::align_of::<$int>());
        assert_eq!(unsafe { core::mem::transmute::<Option<$name>, $int>(None) }, 0);
        assert_eq!(unsafe { core::mem::transmute::<Option<$name>, $int>(Some(VALUE)) }, $value);
        assert_eq!(Some(VALUE).unwrap().get(), $value);
        assert_eq!(VALUE.leading_zeros(), ($value as $int).leading_zeros());
        assert_eq!(VALUE.trailing_zeros(), ($value as $int).trailing_zeros());
        println!("{} {:?}", VALUE, VALUE);
    }};
}
fn main() {
    check!(NonZeroI8,i8,i8::MIN);
    check!(NonZeroI16,i16,i16::MIN);
    check!(NonZeroI32,i32,i32::MIN);
    check!(NonZeroI64,i64,i64::MIN);
    check!(NonZeroI128,i128,i128::MIN);
    check!(NonZeroIsize,isize,isize::MIN);
    check!(NonZeroU8,u8,u8::MAX);
    check!(NonZeroU16,u16,u16::MAX);
    check!(NonZeroU32,u32,u32::MAX);
    check!(NonZeroU64,u64,u64::MAX);
    check!(NonZeroU128,u128,u128::MAX);
    check!(NonZeroUsize,usize,usize::MAX);
}
