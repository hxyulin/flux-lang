use std::borrow::Cow;
fn string_view<'a>(value: &'a Cow<'_, str>) -> &'a str { value.as_ref() }
fn slice_view<'a>(value: &'a Cow<'_, [u32]>) -> &'a [u32] { value.as_ref() }
fn main() {
    let original = String::from("RUSTé🦀");
    let mut borrowed: Cow<'_, str> = Cow::Borrowed(&original);
    let owned: Cow<'_, str> = Cow::Owned(String::from("OWNED中"));
    assert_eq!(string_view(&borrowed), "RUSTé🦀");
    assert_eq!(string_view(&owned), "OWNED中");
    let a: &str = borrowed.as_ref();
    assert_eq!(a.as_ptr(), original.as_ptr());
    borrowed.to_mut().push('!');
    assert_eq!(borrowed.as_ref(), "RUSTé🦀!");
    assert_eq!(original, "RUSTé🦀");
    let array = [1u32, 2, 3];
    let mut borrowed_slice: Cow<'_, [u32]> = Cow::Borrowed(&array);
    let owned_slice: Cow<'_, [u32]> = Cow::Owned(vec![4, 5, 6, 7]);
    assert_eq!(slice_view(&borrowed_slice), &array);
    assert_eq!(slice_view(&owned_slice), &[4, 5, 6, 7]);
    assert_eq!(borrowed_slice.as_ref().as_ptr(), array.as_ptr());
    borrowed_slice.to_mut().push(9);
    assert_eq!(borrowed_slice.as_ref(), &[1, 2, 3, 9]);
    assert_eq!(array, [1, 2, 3]);
    println!("{} {} {:?} {:?}", string_view(&borrowed), string_view(&owned), slice_view(&borrowed_slice), slice_view(&owned_slice));
}
