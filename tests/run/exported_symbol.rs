#[unsafe(no_mangle)]
extern "C" fn flux_export_regression(value: u32) -> u32 { value + 5 }
unsafe extern "C" {
    #[link_name = "flux_export_regression"]
    fn imported(value: u32) -> u32;
}
struct Payload(u64, u64, u64);

#[unsafe(no_mangle)]
fn flux_export_rust(payload: Payload) -> Payload {
    Payload(payload.2 + 5, payload.0, payload.1)
}

unsafe extern "Rust" {
    #[link_name = "flux_export_rust"]
    fn imported_rust(payload: Payload) -> Payload;
}

fn main() {
    // SAFETY: all declarations match the ABI and type of their definitions.
    println!("{}", unsafe { imported(7) });
    let direct = unsafe { imported_rust(Payload(3, 4, 5)) };
    assert_eq!(direct.0, 10);
    let call = std::hint::black_box(imported_rust as unsafe fn(Payload) -> Payload);
    // SAFETY: the declaration and definition use the same Rust ABI and type.
    let Payload(a, b, c) = unsafe { call(Payload(1, 2, 3)) };
    println!("{a} {b} {c}");
}
