trait Zone { type Offset: std::fmt::Debug; }
struct Utc;
#[derive(Debug)]
struct Offset;
impl Zone for Utc { type Offset = Offset; }
impl std::fmt::Display for Offset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("display offset")
    }
}
struct Date<T:Zone>(T::Offset);
impl<T:Zone> std::fmt::Display for Date<T> where T::Offset: std::fmt::Display {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
fn main() {
    let value = Date::<Utc>(Offset);
    println!("{} {:?}",value,value.0);
    assert_eq!(value.to_string(),"display offset");
}
