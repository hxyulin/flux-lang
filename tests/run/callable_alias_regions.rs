type StaticInput = &'static str;
type Pair<'a> = (&'a str, &'a str);
fn static_call(f: &dyn Fn(StaticInput) -> usize) -> usize { f("fixed") }
fn pair_call<'a>(f: &dyn Fn(Pair<'a>) -> usize, x: &'a str, y: &'a str) -> usize { f((x, y)) }
fn main() {
    let first = String::from("abc");
    let second = String::from("defgh");
    let size = |value: StaticInput| value.len();
    let pair_size = |value: Pair<'_>| value.0.len() + value.1.len();
    assert_eq!(static_call(&size), 5);
    assert_eq!(pair_call(&pair_size, &first, &second), 8);
    println!("callable aliases kept static and shared input lifetimes");
}
