fn add(value: i32) -> i32 { value + 7 }
unsafe fn negate(value: i32) -> i32 { -value }
extern "C" fn multiply(value: i32) -> i32 { value * 3 }

static POINTER: fn(i32) -> i32 = add;
static CALLBACK_REF: &fn(i32) -> i32 = &POINTER;
static C_POINTER: extern "C" fn(i32) -> i32 = multiply;
static C_REF: &extern "C" fn(i32) -> i32 = &C_POINTER;

fn callback() -> &'static fn(i32) -> i32 { &POINTER }
fn forward(pointer: &&fn(i32) -> i32) -> i32 { pointer(13) }

fn main() {
    println!("borrowed {} {} {}", callback()(3), CALLBACK_REF(5), forward(&CALLBACK_REF));
    let mut pointer: fn(i32) -> i32 = add;
    let mut reference = &mut pointer;
    println!("mutable {}", (&mut reference)(17));
    println!("c {} {}", C_REF(7), (&&C_POINTER)(11));
    let unsafe_pointer: unsafe fn(i32) -> i32 = negate;
    println!("unsafe {}", unsafe { (&unsafe_pointer)(19) });
    let unsafe_from_safe: Option<unsafe fn(i32) -> i32> = Some(|value| value + 31);
    println!("unsafe-coercion {}", unsafe { unsafe_from_safe.unwrap()(23) });
}
