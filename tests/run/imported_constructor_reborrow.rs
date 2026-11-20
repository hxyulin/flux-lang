mod private_field {
    pub struct Fixed(u16);
}

struct Fixed(u8);

fn named_import() -> u8 {
    use private_field::Fixed;
    let _: Option<Fixed> = None;
    Fixed(5).0
}

fn glob_import() -> u8 {
    use private_field::*;
    let _: Option<Fixed> = None;
    Fixed(11).0
}

fn shared(value: &mut u8) -> Option<&u8> {
    let reference = Some(value as _);
    reference
}

fn main() {
    let mut n = named_import() + glob_import();
    println!("{:?}", shared(&mut n));
    n += 1;
    println!("{n}");
}
