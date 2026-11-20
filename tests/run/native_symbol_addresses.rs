#[unsafe(no_mangle)]
static mut FLUX_TEST_EXPORTED_DATA: u32 = 7;

macro_rules! symbol_suffix {
    () => {
        "RENAMED_DATA"
    };
}

#[unsafe(export_name = concat!("FLUX_TEST_", symbol_suffix!()))]
static RENAMED: u32 = 19;

unsafe extern "C" {
    #[link_name = "FLUX_TEST_EXPORTED_DATA"]
    static mut IMPORTED_ALIAS: u32;
    #[link_name = "FLUX_TEST_RENAMED_DATA"]
    static RENAMED_ALIAS: u32;
}

fn callback<T>(value: T) -> T {
    value
}

fn address(value: *const ()) -> usize {
    value as usize
}

fn integer(value: usize) -> usize {
    value
}

fn main() {
    let from_pointer = address(callback::<u32> as _);
    let from_integer = integer(callback::<u32> as _);
    assert_ne!(from_pointer, 0);
    assert_eq!(from_pointer, from_integer);

    let before = unsafe { IMPORTED_ALIAS };
    unsafe {
        FLUX_TEST_EXPORTED_DATA = 11;
    }
    let after = unsafe { IMPORTED_ALIAS };
    assert_eq!(core::ptr::addr_of!(RENAMED), unsafe {
        core::ptr::addr_of!(RENAMED_ALIAS)
    });
    println!("{before} {after} {}", unsafe { RENAMED_ALIAS });
}
