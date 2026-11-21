const LOW: usize = 44usize.isolate_lowest_one();

fn main() {
    println!("{LOW}");
    for value in [0, 1, 60, -36, i8::MIN, i8::MAX] {
        println!("{}", value.isolate_lowest_one());
    }
    for value in [0, 1, 60, u128::MAX, 1 << 127] {
        println!("{}", value.isolate_lowest_one());
    }
}
