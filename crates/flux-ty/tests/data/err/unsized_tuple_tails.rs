// Only a tuple's last element may be unsized, and a value of a tuple with
// an unsized tail is itself unsized.
struct Lead<T: ?Sized>(std::marker::PhantomData<(T, u8)>);

fn tail<T: ?Sized>(_: &(u8, T)) {}

fn by_value<T: ?Sized>(x: (u8, T)) {}

fn main() {}
