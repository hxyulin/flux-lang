use std::sync::atomic::{AtomicU32, Ordering};

static CALLS: AtomicU32 = AtomicU32::new(0);

unsafe extern "C" fn initialize() {
    CALLS.fetch_add(1, Ordering::Relaxed);
}

#[used]
#[cfg_attr(
    target_vendor = "apple",
    unsafe(link_section = "__DATA,__mod_init_func,mod_init_funcs")
)]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".init_array"))]
static INITIALIZERS: [unsafe extern "C" fn(); 2] = [initialize, initialize];

#[used]
#[cfg_attr(target_vendor = "apple", unsafe(link_section = "__DATA,__flux_test"))]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".data.flux_test"))]
static RECORD: u32 = 17;

fn main() {
    assert_eq!(CALLS.load(Ordering::Relaxed), 2);
    let initializers = std::hint::black_box(&INITIALIZERS);
    unsafe { initializers[0]() };
    assert_eq!(CALLS.load(Ordering::Relaxed), 3);
    assert_eq!(*std::hint::black_box(&RECORD), 17);
}
