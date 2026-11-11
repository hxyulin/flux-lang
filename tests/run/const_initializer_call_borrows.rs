struct Name(&'static str);
impl Name {
    const fn get(&self) -> &str {
        self.0
    }
}
const NAME: Name = Name("field");
struct Fields {
    names: &'static [&'static str],
}
impl Fields {
    const fn new(names: &'static [&'static str]) -> Self {
        Self { names }
    }
}
static FIELDS: Fields = Fields::new(&[NAME.get()]);
const fn value() -> u32 {
    37
}
const fn borrow(values: &'static [u32]) -> &'static [u32] {
    values
}
const NUMBERS: &[u32] = borrow(&[value()]);
fn main() {
    println!("{} {}", FIELDS.names[0], NUMBERS[0]);
}
