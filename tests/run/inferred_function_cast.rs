fn identity<T>(value: T) -> T { value }
fn project<T>((_, value): (usize, T)) -> T { value }
fn map<'a>(values: &'a [u32]) -> std::iter::Map<std::iter::Enumerate<std::slice::Iter<'a, u32>>, fn((usize, &'a u32)) -> &'a u32> {
    values.iter().enumerate().map(project as _)
}
unsafe fn unsafe_identity(value: u32) -> u32 { value }
extern "C" fn c_identity(value: u32) -> u32 { value }
fn main() {
    let pointer: fn(u32) -> u32 = identity as _;
    assert_eq!(pointer(17), 17);
    let copy: fn(u32) -> u32 = pointer as _;
    assert_eq!(copy(19), 19);
    let unsafe_pointer: unsafe fn(u32) -> u32 = identity as _;
    // SAFETY: identity has no preconditions.
    assert_eq!(unsafe { unsafe_pointer(23) }, 23);
    let unsafe_pointer: unsafe fn(u32) -> u32 = unsafe_identity as _;
    // SAFETY: unsafe_identity has no preconditions.
    assert_eq!(unsafe { unsafe_pointer(29) }, 29);
    let c_pointer: extern "C" fn(u32) -> u32 = c_identity as _;
    assert_eq!(c_pointer(31), 31);
    let values = [7, 11, 13];
    assert_eq!(map(&values).copied().sum::<u32>(), 31);
    println!("inferred function casts complete");
}
