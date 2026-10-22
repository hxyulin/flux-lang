//! Built-in Box dereferences give argument temporaries the coerced type,
//! including chains through nested boxes and temporary references.

fn read(value: &i32) -> i32 {
    *value
}

fn read_boxes(single: &Box<i32>, nested: &Box<Box<i32>>) {
    println!("{} {}", read(single), read(nested));
}

fn main() {
    let single = Box::new(7);
    let nested = Box::new(Box::new(11));
    read_boxes(&single, &nested);
    println!("{} {}", read(&single), read(&nested));
    println!("{}", read(&Box::new(Box::new(13))));
}
