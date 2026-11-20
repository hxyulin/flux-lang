use core::ops::Deref;

struct Data<'bytes, 'fds>(&'bytes [u8], &'fds ());

impl Deref for Data<'_, '_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: ?Sized> AsRef<T> for Data<'_, '_>
where
    for<'bytes, 'fds> <Data<'bytes, 'fds> as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

fn main() {
    let bytes = vec![7u8];
    let value = Data(&bytes, &());
    let result: &[u8] = value.as_ref();
    println!("{}", result[0]);
}
