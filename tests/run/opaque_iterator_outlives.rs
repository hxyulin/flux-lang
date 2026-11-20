fn iter<'a, T>(bytes: &'a [u8]) -> impl Iterator<Item = Result<T, u8>> + 'a {
    let mut bytes = bytes.iter();
    std::iter::from_fn(move || bytes.next().map(|&byte| Err(byte)))
}

fn mapped<'a, T>(bytes: &'a [u8]) -> impl Iterator<Item = Option<Result<T, u8>>> + 'a {
    iter(bytes).map(|item| match item {
        Err(0) => None,
        other => Some(other),
    })
}

fn main() {
    let bytes = [1, 0, 3];
    let result: Vec<_> = mapped::<&str>(&bytes).collect();
    println!("{result:?}");
}
