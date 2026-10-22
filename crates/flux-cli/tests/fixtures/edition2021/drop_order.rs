struct D(u8);
impl Drop for D {
    fn drop(&mut self) {
        println!("drop={}", self.0);
    }
}
fn make() -> D {
    D(2)
}
fn value() -> u8 {
    let _local = D(1);
    make().0
}
fn main() {
    println!("value={}", value());
}
