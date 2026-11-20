trait Value {
    fn read(&self) -> u32;
}

trait Container {
    fn value(&self) -> &(impl Value + 'static);
}

struct Number(u32);
struct Panel(Number);

impl Value for Number {
    fn read(&self) -> u32 {
        self.0
    }
}

impl Container for Panel {
    fn value(&self) -> &(impl Value + 'static) {
        &self.0
    }
}

fn main() {
    let panel = Panel(Number(42));
    println!("{}", panel.value().read());
}
