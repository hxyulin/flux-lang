static EMPTY: Vec<u32> = Vec::new();
const EMPTY_VIEW: &[u32] = EMPTY.as_slice();

trait Edit<'a> {}
struct Editor;
struct Buffer;

impl<'a> Edit<'a> for Editor {}

struct Borrowed<T>(T);

impl<'a, T: Edit<'a>> Borrowed<T> {
    fn width(&self) -> u16 {
        17
    }
}

impl Borrowed<Buffer> {
    fn width(&self) -> u16 {
        29
    }
}

const fn view(values: &Vec<u32>) -> &[u32] {
    values.as_slice()
}

fn main() {
    let mut values = Vec::with_capacity(4);
    println!(
        "{} {}",
        EMPTY_VIEW.len(),
        std::ptr::eq(values.as_ptr(), values.as_slice().as_ptr())
    );
    values.push(17);
    println!("{:?}", view(&values));
    println!("{} {}", Borrowed(Editor).width(), Borrowed(Buffer).width());

    let mut map = std::collections::HashMap::new();
    let inserted = map
        .entry(String::from("entry"))
        .or_insert_with_key(|key| key.len());
    *inserted += 1;
    let existing = map
        .entry(String::from("entry"))
        .or_insert_with_key(|_| panic!("the entry is occupied"));
    println!("{existing}");
}
