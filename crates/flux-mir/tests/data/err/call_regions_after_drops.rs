fn id<T>(x: T) -> T { x }
fn main() {
    let mut v = Vec::new();
    v.push(String::new());
    let s = String::from("a");
    let r = id::<&'static str>(&s);
    println!("{r} {}", v.len());
}
