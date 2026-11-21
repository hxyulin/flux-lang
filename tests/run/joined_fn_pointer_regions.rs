struct Buffer<'a>(&'a u32);
type Stream<'a> = &'a Buffer<'a>;

fn read(input: Stream<'_>) -> u32 {
    *input.0
}

fn increment(input: Stream<'_>) -> u32 {
    *input.0 + 1
}

fn parse(input: Stream<'_>, next: bool) -> u32 {
    let parser = match next {
        false => read,
        true => increment,
    };
    parser(input)
}

fn main() {
    let number = 41;
    let input = Buffer(&number);
    println!("{} {}", parse(&input, false), parse(&input, true));
}
