static VALUE: u32 = 42;

unsafe fn read_static(pointer: *const &u32) -> &'static u32 {
    // SAFETY: callers supply a live pointer containing a reference to VALUE.
    unsafe { *(pointer as *const &'static u32) }
}

unsafe fn read_named<'a>(pointer: *const &u32) -> &'a u32 {
    // SAFETY: callers supply a live pointer containing a reference to VALUE.
    unsafe { *(pointer as *const &'a u32) }
}

fn main() {
    let reference = &VALUE;
    let pointer = &reference as *const &u32;
    // SAFETY: pointer addresses reference throughout both calls, and VALUE is static.
    let (first, second) = unsafe { (read_static(pointer), read_named(pointer)) };
    assert_eq!(*first, 42);
    assert_eq!(*second, 42);
    println!("{} {}", first, second);
}
