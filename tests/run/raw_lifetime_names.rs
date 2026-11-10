fn borrow<'r#name>(value: &'name str) -> &'r#name str {
    value
}
fn keyword<'r#loop>(value: &'r#loop str) -> &'r#loop str {
    value
}
fn static_borrow(value: &'r#static str) -> &'static str {
    value
}
macro_rules! make {
    ($l:lifetime) => {fn generated<$l>(value: &'name str) -> &$l str { value }};
}
make!('r#name);
fn main() {
    let value = String::from("borrowed");
    assert_eq!(borrow(&value), "borrowed");
    assert_eq!(generated(&value), "borrowed");
    assert_eq!(keyword(&value), "borrowed");
    assert_eq!(static_borrow("static"), "static");
    let result = 'r#label: loop {
        break 'label 7;
    };
    assert_eq!(result, 7);
    let result = 'r#loop: loop {
        break 'r#loop 11;
    };
    assert_eq!(result, 11);
    println!("raw lifetime and label names passed");
}
