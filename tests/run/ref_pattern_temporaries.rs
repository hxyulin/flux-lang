struct Value(&'static str, u32);

impl Drop for Value {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    {
        let (ref left, ref mut right) = (Value("left", 13), Value("right", 28));
        right.1 += 1;
        assert_eq!(left.1 + right.1, 42);
        println!("{}", left.1 + right.1);
    }
    println!("done");
}
