struct Record<'a>(&'a u8);
impl<'a> Record<'a> {
    fn at(data: &'a [u8], i: usize) -> Self {
        Self(&data[i])
    }
    fn values(data: &'a [u8]) -> impl Iterator<Item = Self> + 'a + Clone {
        (0..data.len()).map(|i| Record::at(data, i))
    }
}

fn display<'a, 'b: 'a>(_: &'a str, value: &'b str) -> impl std::fmt::Display + use<'a> {
    value
}

fn main() {
    let data = [1, 2, 3];
    println!("{}", Record::values(&data).next().unwrap().0);
    let local = String::from("short");
    println!("{}", display(&local, "long"));
}
