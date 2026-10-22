struct Holder {
    p: *mut u8,
}

static mut COUNT: u32 = 0;
static PLACEHOLDER: _ = 5;
static POINTER: Option<*const i32> = None;
static HOLDER: Option<Holder> = None;
static mut MUT_POINTER: Option<*const i32> = None;

fn not_const() -> u32 {
    1
}

static CALLS: u32 = not_const();

fn main() {
    let a = COUNT;
    COUNT = 1;
    COUNT += 1;
    let b = unsafe { COUNT };
}
