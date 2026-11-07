fn doubled(value: usize) -> usize { value * 2 }
fn main() {
    let pair = ((0usize..4).map(doubled as fn(_) -> _), true);
    println!("{} {}", pair.0.sum::<usize>(), pair.1);
}
