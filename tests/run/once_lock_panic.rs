use std::sync::OnceLock;
fn main() {
    let lock: OnceLock<usize> = OnceLock::new();
    lock.get_or_init(|| panic!("once-lock initializer failed"));
}
