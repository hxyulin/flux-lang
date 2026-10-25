fn identity<T>(value: T) -> T { value }
fn increment(value: i32) -> i32 { value + 1 }
const fn pair(f: fn(i32) -> i32) -> [fn(i32) -> i32; 2] { [f, increment] }
const ID: fn(u32) -> u32 = identity::<u32>;
static FUNCTIONS: [fn(i32) -> i32; 2] = pair(identity::<i32>);
static REFERENCE: &fn(u32) -> u32 = &ID;
struct Holder { call: fn(u32) -> u32 }
static HOLDER: Holder = Holder { call: identity::<u32> };
struct Generic<T>(T);
struct Callback<T> { call: fn(T) -> T }
impl<T> Generic<T> {
    const CALL: fn(T) -> T = identity::<T>;
    const WRAPPED: Callback<T> = Callback { call: identity::<T> };
    const ARRAY: [fn(T) -> T; 1] = [identity::<T>];
}
fn indirect<T>(value: T) -> T { (Generic::<T>::CALL)(value) }

// Each successive pointee changes an associated field from scalar to callback.
trait Data { type Out; }
struct Start;
struct Next<T>(T);
impl Data for Start { type Out = u32; }
impl<T> Data for Next<T> { type Out = fn() -> u32; }
struct Link<T: Data> { data: T::Out, next: *const Link<Next<T>> }
fn target<T>() -> u32 { 42 }
struct Graph<T>(T);
impl<T> Graph<T> {
    const VALUE: Link<Start> = Link {
        data: 0,
        next: &Link::<Next<Start>> { data: target::<T>, next: std::ptr::null() },
    };
}

fn main() {
    // SAFETY: the pointer targets a promoted immutable constant allocation.
    assert_eq!(unsafe { ((*Graph::<u32>::VALUE.next).data)() }, 42);
    assert_eq!(indirect(42u32), 42);
    assert_eq!((Generic::<u32>::WRAPPED.call)(43), 43);
    assert_eq!(Generic::<u32>::ARRAY[0](44), 44);
    assert_eq!(indirect(-7i64), -7);
    assert_eq!(ID(17), 17);
    assert_eq!(FUNCTIONS[0](-3), -3);
    assert_eq!(FUNCTIONS[1](41), 42);
    assert_eq!((*REFERENCE)(23), 23);
    assert_eq!((HOLDER.call)(29), 29);
    println!("functions 17 -3 42 23 29");
}
