// Function pointer types overlap structurally.
trait Tr {}
impl<T> Tr for fn(T) {}
impl Tr for fn(u8) {}

trait Other {}
impl<T> Other for fn() -> T {}
impl Other for unsafe fn() -> u8 {}
impl Other for extern "C" fn() -> u8 {}

fn main() {}
