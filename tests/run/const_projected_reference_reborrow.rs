struct Flag { name: &'static str, bits: u32 }
impl Flag {
    const fn name(&self) -> &'static str { self.name }
}
const FLAGS: &[Flag] = &[
    Flag { name: "read", bits: 1 },
    Flag { name: "write", bits: 2 },
];
const SECOND: &str = FLAGS[1].name();
const FIRST: &str = FLAGS[0].name();
fn main() {
    assert_eq!(FIRST, "read");
    assert_eq!(SECOND, "write");
    println!("{} {} {}", FIRST, SECOND, FLAGS[1].bits);
}
