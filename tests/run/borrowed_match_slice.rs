fn fields(choice: u32) -> usize {
    let values: &[_] = &match choice {
        0 => Vec::new(),
        1 => vec![4, 7],
        _ => (0..3).collect(),
    };
    values.iter().sum()
}
fn main() {
    let arrays: &[(&str, &[&str])] = &[("a", &["b"]), ("c", &["d", "e"])];
    println!("{} {} {} {}", fields(0), fields(1), fields(2), arrays[1].1.len());
}
