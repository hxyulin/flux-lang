struct Owner(u32);

impl Drop for Owner {
    fn drop(&mut self) {
        println!("owner {}", self.0);
    }
}

struct Borrowed<'a>(&'a Owner);

impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        println!("borrowed {}", self.0.0);
    }
}

impl Owner {
    fn borrow(&self) -> Borrowed<'_> {
        Borrowed(self)
    }
}

impl Borrowed<'_> {
    fn value(&self) -> u32 {
        self.0.0
    }
}

fn main() {
    let block = match Some(Owner(1)) {
        Some(owner) => owner.borrow().value(),
        None => 0,
    };
    let expression = match Some(Owner(2)) {
        Some(owner) => owner.borrow().value(),
        None => 0,
    };
    println!("{block} {expression}");
}
