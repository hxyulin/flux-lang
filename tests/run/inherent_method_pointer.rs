struct Label<'a>(&'a str);

impl<'a> Label<'a> {
    fn full(&self) -> &'a str {
        self.0
    }

    fn tail(&self) -> &'a str {
        &self.0[1..]
    }

    fn choose(&self, full: bool) -> &'a str {
        let select = if full { Self::full } else { Self::tail };
        select(self)
    }

    fn typed(&self) -> &'a str {
        let select: fn(&Self) -> &'a str = Self::tail;
        select(self)
    }
}

fn main() {
    let text = String::from("abc");
    let label = Label(&text);
    println!("{} {} {}", label.choose(true), label.choose(false), label.typed());
}
