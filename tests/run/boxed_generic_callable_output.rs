struct Label<'a> {
    text: &'a str,
}

fn erase<'a>(callback: impl Fn(usize) -> Label<'a> + 'a) -> Box<dyn Fn(usize) -> Label<'a> + 'a> {
    Box::new(callback)
}

fn main() {
    let title = String::from("from caller");
    let label = erase(|_| Label { text: &title });
    println!("{}", label(0).text);
}
