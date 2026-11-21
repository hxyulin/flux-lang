trait Reflect {
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, ()>;
    fn take<T: 'static>(&self) -> Result<T, ()>
    where
        Self: Sized,
    {
        Err(())
    }
}
impl Reflect for u8 {
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, ()> {
        Ok(Box::new(*self))
    }
}
impl<T: Reflect + 'static> Reflect for Option<T>
where
    Option<T>: std::any::Any + Send + Sync,
{
    fn reflect_clone(&self) -> Result<Box<dyn Reflect>, ()> {
        Ok(Box::new(match self {
            Option::None {} => Option::None {},
            Option::Some { 0: item } => Option::Some {
                0: <T as Reflect>::take(item)?,
            },
        }))
    }
}
fn main() {
    assert!(None::<u8>.reflect_clone().is_ok());
    assert!(Some(7u8).reflect_clone().is_err());
}
