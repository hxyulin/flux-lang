use std::num::{NonZeroI32, NonZeroU64};
fn main() {
    let signed = NonZeroI32::new(-4200).unwrap();
    let unsigned = NonZeroU64::new(4200).unwrap();
    println!("{:e} {:E} {:.2e}", signed, unsigned, signed);
}
