//! `extern "system"`, `"C-unwind"`, `"system-unwind"` and `"efiapi"`
//! (aarch64) functions use the C calling convention on flux's targets, in
//! definitions, `extern` blocks and function pointers. Each ABI is its own
//! function pointer type, so traits can be implemented for each of them.

unsafe extern "C-unwind" {
    fn abs(x: i32) -> i32;
}

unsafe extern "system" {
    fn labs(x: i64) -> i64;
}

extern "system" fn add(a: u32, b: u32) -> u32 {
    a + b
}

extern "C-unwind" fn mul(a: u32, b: u32) -> u32 {
    a * b
}

extern "system-unwind" fn sub(a: u32, b: u32) -> u32 {
    a - b
}

extern "Rust" fn rust(a: u32) -> u32 {
    a + 100
}

#[cfg(target_arch = "aarch64")]
extern "efiapi" fn efi(a: u32) -> u32 {
    a << 1
}

#[cfg(not(target_arch = "aarch64"))]
extern "C" fn efi(a: u32) -> u32 {
    a << 1
}

trait Name {
    fn name() -> &'static str;
}

impl Name for extern "C" fn(u32, u32) -> u32 {
    fn name() -> &'static str {
        "C"
    }
}

impl Name for extern "C-unwind" fn(u32, u32) -> u32 {
    fn name() -> &'static str {
        "C-unwind"
    }
}

impl Name for extern "system" fn(u32, u32) -> u32 {
    fn name() -> &'static str {
        "system"
    }
}

fn name_of<F: Name>(_: F) -> &'static str {
    F::name()
}

fn call(f: extern "system" fn(u32, u32) -> u32, a: u32, b: u32) -> u32 {
    f(a, b)
}

fn main() {
    let unwind: extern "C-unwind" fn(u32, u32) -> u32 = mul;
    let system_unwind: unsafe extern "system-unwind" fn(u32, u32) -> u32 = sub;
    let rust_ptr: fn(u32) -> u32 = rust;
    println!("{}", call(add, 2, 3));
    println!("{}", unwind(6, 7));
    println!("{}", unsafe { system_unwind(10, 4) });
    println!("{}", rust_ptr(1));
    println!("{}", efi(21));
    println!("{} {}", unsafe { abs(-5) }, unsafe { labs(-9) });
    println!("{}", name_of(unwind));
    println!("{}", name_of(add as extern "system" fn(u32, u32) -> u32));
    std::process::exit(efi(3) as i32);
}
