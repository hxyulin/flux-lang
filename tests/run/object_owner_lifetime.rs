trait View<'a> {}
impl<'a> View<'a> for &'a u32 {}
fn object<'a>(value: &'a u32) -> Box<dyn View<'a> + 'a> { Box::new(value) }
fn main() { let value = 42; let view = object(&value); drop(view); println!("{value}"); }
