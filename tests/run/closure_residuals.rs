use std::ops::ControlFlow;
struct Small(u32);
struct Large(u32);
impl From<Small> for Large { fn from(value: Small) -> Large { Large(value.0 + 100) } }
fn main() {
    let optional = |input: Option<u32>| { let value = input?; Some(value + 1) };
    assert_eq!(optional(Some(7)), Some(8));
    assert_eq!(optional(None), None);
    let converted = |input: Result<u32, Small>| { let value = input?; Ok::<u32, Large>(value + 2) };
    assert_eq!(converted(Ok(11)).ok(), Some(13));
    let error = converted(Err(Small(17))).err().unwrap();
    assert_eq!(error.0, 117);
    let control = |input: ControlFlow<u32, u32>| { let value = input?; ControlFlow::Continue(value + 3) };
    assert_eq!(control(ControlFlow::Continue(19)), ControlFlow::Continue(22));
    assert_eq!(control(ControlFlow::Break(23)), ControlFlow::Break(23));
    let outer = |input: Option<u32>| {
        let inner = |value: Result<u32, u32>| { let value = value?; Ok::<u32, u32>(value + 5) };
        Some(inner(Ok(input?)).ok()?)
    };
    assert_eq!(outer(Some(29)), Some(34));
    assert_eq!(outer(None), None);
    println!("residuals {} {} {}", optional(Some(31)).unwrap(), error.0, outer(Some(37)).unwrap());
}
