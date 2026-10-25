struct Guard;
impl Drop for Guard {
    fn drop(&mut self) { print!("tail"); }
}
thread_local! { static GUARD: Guard = Guard; }
fn main() {
    GUARD.with(|_| ());
    print!("body");
}
