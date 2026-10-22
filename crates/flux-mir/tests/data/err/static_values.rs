static mut COUNT: i32 = 7;
static ZERO: i32 = 0;
static DIVIDED: i32 = 1 / ZERO;
const READS_MUT: i32 = unsafe { COUNT };
// A static's initializer reads another's initial value.
static COPIED: i32 = unsafe { COUNT } + 1;
static OVERFLOWS: u8 = 255 + COPIED as u8;

fn main() {}
