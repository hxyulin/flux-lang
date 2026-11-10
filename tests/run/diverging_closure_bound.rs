fn invoke<F: FnOnce() -> R, R>(function: F) -> Result<R, ()> { Ok(function()) }
fn check() {
    // The diverging output has a fallback before unwrap_err checks Debug.
    let _ = invoke(|| panic!("not executed")).unwrap_err();
}
fn main() {
    let _ = check as fn();
    println!("{}", invoke(|| 17).unwrap());
}
