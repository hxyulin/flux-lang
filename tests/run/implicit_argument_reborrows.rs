fn append(values: &mut Vec<u32>, n: usize) { values.push(n as u32); }
fn modify(value: &mut u32, previous: u32) { *value += previous; }
fn work(values: &mut Vec<u32>) {
    append(values, values.len());
    append(values, values[0] as usize);
}
fn scalar(value: &mut u32) { modify(value, *value); }
fn main() {
    let mut values = vec![7u32, 11];
    work(&mut values);
    assert_eq!(values, [7, 11, 2, 7]);
    let mut value = 13;
    scalar(&mut value);
    assert_eq!(value, 26);
    println!("reborrow {:?} {}", values, value);
}
