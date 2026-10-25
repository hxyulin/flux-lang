use std::{fmt, str::FromStr};
macro_rules! operations {
    ($($variant:ident => $name:literal),* $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Operation { $($variant),* }
        impl FromStr for Operation {
            type Err = ();
            fn from_str(value: &str) -> Result<Self, ()> {
                Ok(match value { $($name => Self::$variant,)* _ => return Err(()) })
            }
        }
        impl fmt::Display for Operation {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(match self { $(Self::$variant => $name,)* })
            }
        }
    };
}
operations! { Cls => "cls", Rbit => "rbit", Trap => "trap" }
enum Value<T> { Empty, Item(T), Record { value: T } }
trait Make<T> { fn make(value: T) -> Self; fn take(self) -> Option<T>; }
impl<T> Make<T> for Value<T> {
    fn make(value: T) -> Self { Self::Item(value) }
    fn take(self) -> Option<T> {
        match self { Self::Empty => None, Self::Item(value) | Self::Record { value } => Some(value) }
    }
}
fn main() {
    for text in ["cls", "rbit", "trap"] {
        let op = Operation::from_str(text).unwrap();
        assert_eq!(op.to_string(), text);
        println!("{} {:?}", op, op);
    }
    assert!(Operation::from_str("missing").is_err());
    assert_eq!(Value::make(String::from("owned")).take().unwrap(), "owned");
    assert_eq!(Value::<u32>::Empty.take(), None);
    assert_eq!(Value::Record { value: 29u32 }.take(), Some(29));
}
