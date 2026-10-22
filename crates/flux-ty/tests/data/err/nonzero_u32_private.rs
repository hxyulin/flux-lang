use std::num::NonZeroU32;

fn main() {
    let mut valid = NonZeroU32::new(1).unwrap();
    valid.0 = 0;
}
