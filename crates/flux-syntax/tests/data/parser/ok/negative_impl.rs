struct S;
#[cfg(any())]
impl !Send for S {}
#[cfg(any())]
impl<T> !Sync for Wrapper<T> {}
