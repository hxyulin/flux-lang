use std::error::Error;
use std::fmt;
#[derive(Debug)]
struct Inner(u32);
impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "inner {}", self.0)
    }
}
impl Error for Inner {}
#[derive(Debug)]
struct Outer {
    source: Option<Box<dyn Error + Send + Sync>>,
}
impl fmt::Display for Outer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "outer")
    }
}
impl Error for Outer {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}
trait Base {
    fn value(&self) -> u32;
}
trait Sub: Base {
    fn add(&self) -> u32;
}
impl Base for Inner {
    fn value(&self) -> u32 {
        self.0
    }
}
impl Sub for Inner {
    fn add(&self) -> u32 {
        self.0 + 1
    }
}
fn base(x: &dyn Sub) -> &dyn Base {
    x as _
}
fn erased(x: &(dyn Base + Send + Sync)) -> &dyn Base {
    x as _
}
fn main() {
    let outer = Outer {
        source: Some(Box::new(Inner(37))),
    };
    assert_eq!(outer.source().unwrap().to_string(), "inner 37");
    let empty = Outer { source: None };
    assert!(empty.source().is_none());
    let value = Inner(42);
    let sub: &dyn Sub = &value;
    assert_eq!(sub.add(), 43);
    assert_eq!(base(sub).value(), 42);
    let marked: &(dyn Base + Send + Sync) = &value;
    assert_eq!(erased(marked).value(), 42);
    println!(
        "inferred casts {} {}",
        outer.source().unwrap(),
        base(sub).value()
    );
}
