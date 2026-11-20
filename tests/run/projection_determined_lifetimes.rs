struct Elements<'a, I: Iterator<Item = &'a u32>>(I);

fn shorten<'a, 'b: 'a>(
    elements: Elements<'b, std::slice::Iter<'b, u32>>,
) -> Elements<'a, std::slice::Iter<'a, u32>> {
    elements
}

fn pair<'a>(
    _: &'a u32,
    available: Elements<'static, std::slice::Iter<'static, u32>>,
) -> Elements<'a, std::slice::Iter<'a, u32>> {
    shorten(available)
}

static VALUES: [u32; 2] = [19, 23];

fn main() {
    let local = 7;
    let elements = pair(&local, Elements(VALUES.iter()));
    println!("{}", elements.0.copied().sum::<u32>());
}
