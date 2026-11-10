struct Raw<'r#mod>(&'r#mod u8);
type Callback = unsafe extern "C" fn(i32, ...) -> i32;
unsafe extern "C" {
    fn variadic(first: i32, ...);
    safe fn count() -> u32;
    safe static COUNT: u32;
}
fn display(_: &dyn (std::fmt::Debug)) {}
fn safe() {}
