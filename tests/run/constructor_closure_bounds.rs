use std::fmt::{self, Display, Formatter};

struct FormatFn<F: Fn(&mut Formatter) -> fmt::Result>(F);
impl<F: Fn(&mut Formatter) -> fmt::Result> Display for FormatFn<F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { (self.0)(f) }
}

fn greeting(value: &str) -> impl Display + '_ {
    FormatFn(move |f| write!(f, "hello {value}"))
}

struct Measure<F: Fn(T) -> usize, T>(F, T);
impl<F: Fn(T) -> usize, T> Measure<F, T> {
    fn run(self) -> usize { (self.0)(self.1) }
}

fn main() {
    let name = String::from("world");
    println!("{}", greeting(&name));
    let task = Measure(|value| value.len(), name);
    println!("{}", task.run());
}
