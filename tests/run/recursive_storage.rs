struct Next<T>(*const T);
struct Link<T> { next: *const Link<T>, value: T }
struct Growing<T> { next: *const Growing<Next<T>>, value: T }
fn same<'a, T>(value: &'a Link<T>) -> &'a T { &value.value }
fn growing<'a, T>(value: &'a Growing<T>) -> &'a T { &value.value }
trait Value { type Stored; }
struct Factory<'a>(&'a u32);
impl<'a> Value for Factory<'a> { type Stored = &'a u32; }
struct Storage<T: Value> { next: *const Storage<T>, stored: T::Stored }
struct Nested<T: Value>(Storage<T>);
fn main() {
    let value = 71u32;
    let first = Link { next: std::ptr::null(), value: &value };
    let second = Growing { next: std::ptr::null(), value: &value };
    let nested = Nested::<Factory<'_>>(Storage { next: std::ptr::null(), stored: &value });
    assert_eq!(**same(&first), 71);
    assert_eq!(**growing(&second), 71);
    assert_eq!(*nested.0.stored, 71);
    println!("recursive {} {} {}", same(&first), growing(&second), nested.0.stored);
}
