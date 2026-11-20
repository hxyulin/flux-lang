fn read(number: &u32) -> u32 {
    *number
}

fn getter<'a>(number: &'a u32) -> impl Fn() -> u32 + 'a {
    || read(number)
}

struct Reader;

impl Reader {
    fn add(&self, number: &u32, offset: u32) -> u32 {
        *number + offset
    }
}

static READER: Reader = Reader;

fn offsets<'a>(number: &'a u32) -> impl Iterator<Item = u32> + 'a {
    [0, 1]
        .into_iter()
        .filter_map(|offset| Some(READER.add(number, offset)))
}

fn main() {
    let number = 17;
    println!(
        "{} {:?}",
        getter(&number)(),
        offsets(&number).collect::<Vec<_>>()
    );
}
