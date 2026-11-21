fn first<T: AsRef<str>>(value: T, output: &mut [usize]) -> usize {
    output[0] = value.as_ref().len();
    output[0]
}
fn second<T: AsRef<str>>(value: T, output: &mut [usize]) -> usize {
    output[0] = value.as_ref().len() + 1;
    output[0]
}

fn run(mut callback: impl FnMut(&mut [usize]) -> usize) -> usize {
    let mut a = [0];
    let mut b = [0];
    callback(&mut a) + callback(&mut b)
}

fn main() {
    let flag = std::hint::black_box(false);
    let choose = if flag { first } else { second };
    let text = String::from("local");
    println!("{}", run(|output| choose(text.as_str(), output)));
}
