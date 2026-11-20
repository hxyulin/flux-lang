#[derive(Debug)]
struct First(u32);
#[derive(Debug)]
struct Other;
impl std::fmt::Display for First {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "first {}", self.0)
    }
}
impl std::fmt::Display for Other {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("other")
    }
}
impl std::error::Error for First {}
impl std::error::Error for Other {}

#[derive(Debug)]
struct Wrapped(First);

impl std::fmt::Display for Wrapped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wrapped {}", self.0)
    }
}

impl std::error::Error for Wrapped {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

fn print_source(error: impl std::error::Error) {
    println!(
        "{}",
        error.source().unwrap().downcast_ref::<First>().unwrap()
    );
}

fn main() {
    let mut x = First(7);
    let e: &mut (dyn std::error::Error + 'static) = &mut x;
    assert!(e.is::<First>());
    assert!(!e.is::<Other>());
    assert!(e.downcast_ref::<Other>().is_none());
    e.downcast_mut::<First>().unwrap().0 = 9;
    println!("{}", e.downcast_ref::<First>().unwrap());
    print_source(&Wrapped(First(12)));
}
