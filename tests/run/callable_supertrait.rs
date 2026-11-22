struct Data(u32);

trait ResolveFn: Fn(&mut Data) -> Result<(), String> {}

impl<T: Fn(&mut Data) -> Result<(), String>> ResolveFn for T {}

fn require<T: ResolveFn>(resolve: T) -> T {
    resolve
}

fn main() {
    let mut data = Data(7);
    let resolve = require(|value: &mut Data| {
        value.0 += 1;
        Ok(())
    });
    resolve(&mut data).unwrap();
    println!("{}", data.0);
}
