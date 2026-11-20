trait Value {
    fn get(&self) -> u32;
}

struct Number(u32);
impl Value for Number {
    fn get(&self) -> u32 {
        self.0
    }
}

fn read(value: Option<&dyn Value>) -> u32 {
    value.unwrap().get()
}

fn main() {
    let value = Some(Number(42));
    let dynamic = value.as_ref().map(|number| number as &_);
    println!("{}", read(dynamic));
    let inferred = &17u32 as &_;
    println!("{inferred}");
}
