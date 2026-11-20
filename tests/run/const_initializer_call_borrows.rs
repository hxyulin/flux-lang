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

#[derive(Clone, Copy)]
enum Encoding {
    Word,
    Small,
    Object,
    Pointer(&'static Encoding),
    Record(&'static [Encoding]),
}

impl Encoding {
    const TARGET_WORD: Self = if cfg!(target_pointer_width = "64") {
        Self::Word
    } else {
        Self::Small
    };
}

const ENCODING: Encoding = Encoding::Record(&[
    Encoding::TARGET_WORD,
    Encoding::Pointer(&Encoding::Object),
    Encoding::Object,
]);

fn main() {
    println!("{} {}", FIELDS.names[0], NUMBERS[0]);
    let Encoding::Record(fields) = ENCODING else {
        panic!("expected a record");
    };
    assert_eq!(fields.len(), 3);
    assert!(matches!(fields[1], Encoding::Pointer(&Encoding::Object)));
    assert!(matches!(fields[2], Encoding::Object));
}
