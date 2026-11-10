fn runtime() -> u32 {
    9
}
const fn unused() -> u32 {
    loop {}
    runtime()
}
fn main() {
    println!("unreachable const call accepted");
}
