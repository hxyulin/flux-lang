trait Family<'a> {
    type Output;
}
impl<'a> Family<'a> for () {
    type Output = &'a [u8];
}

fn apply<Y, F>(f: F) -> u8
where
    for<'a> Y: Family<'a>,
    F: FnOnce(&[u8]) -> <Y as Family>::Output,
    for<'a> <Y as Family<'a>>::Output: AsRef<[u8]>,
{
    let bytes = [17, 23];
    f(&bytes).as_ref()[1]
}

fn main() {
    assert_eq!(apply::<(), _>(|bytes| bytes), 23);
}
