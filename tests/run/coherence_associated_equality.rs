struct Record;

trait Repr {
    type Output;
}

impl Repr for u8 {
    type Output = u8;
}

impl Repr for Record {
    type Output = Record;
}

trait Abi {
    const TAG: u8;

    fn tag(&self) -> u8;
}

impl Abi for u8 {
    const TAG: u8 = 3;

    fn tag(&self) -> u8 {
        1
    }
}

impl<T: Repr<Output = Record>> Abi for T {
    const TAG: u8 = 4;

    fn tag(&self) -> u8 {
        2
    }
}

fn main() {
    println!("{} {}", 0u8.tag(), Record.tag());
    println!("{} {}", <u8 as Abi>::TAG, <Record as Abi>::TAG);
}
