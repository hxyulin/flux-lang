fn shared<F: Fn(u8) -> u8>(f: F) -> u8 { f(1) }
fn repeat<F: FnMut()>(mut f: F) { f(); }
fn wrong_mutation() {
    let mut value = 0u8;
    let _ = shared({ |x| { value += x; value } });
}
fn wrong_move() {
    let value = String::from("owned");
    repeat({ move || drop(value) });
}
