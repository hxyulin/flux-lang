fn convert(value: u16) -> u16 {
    let scale: u16 = 10000.try_into().expect("conversion");
    if value > 999.try_into().expect("conversion") { value % scale } else { value }
}
fn main() {
    let a: u16 = 1234.try_into().expect("conversion");
    let b = u16::try_from(9999).expect("conversion");
    assert_eq!(convert(a), 1234);
    assert_eq!(convert(b), 9999);
    println!("{} {} {}", convert(a), convert(b), convert(17));
}
