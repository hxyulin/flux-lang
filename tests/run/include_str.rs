const TEXT: &str = include_str!("support/include_str/data.txt");
static STATIC: &str = include_str!(r#"support/include_str/data.txt"#,);
fn returned() -> &'static str {
    include_str!("support/include_str/data.txt")
}
#[cfg(any())]
const DISABLED: &str = include_str!("absent");
fn main() {
    println!("{:?}", TEXT.as_bytes());
    println!("{:?}", STATIC.as_bytes());
    println!("{:?}", returned().as_bytes());
}
