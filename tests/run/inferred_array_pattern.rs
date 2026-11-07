fn main() {
    let values = [2, 3, 5];
    let pair = matches!(values.last_chunk(), Some([a, b]) if *a == 3 && *b == 5);
    let empty = matches!(values.first_chunk(), Some([]));
    println!("{} {}", pair, empty);
}
