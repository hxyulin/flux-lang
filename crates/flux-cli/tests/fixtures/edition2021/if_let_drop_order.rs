struct D;
impl Drop for D {
    fn drop(&mut self) {
        println!("drop");
    }
}
fn make() -> (Option<u8>, D) {
    (None, D)
}
fn main() {
    if let (Some(_), _) = make() {
        println!("then");
    } else {
        println!("else");
    }
}
