const VALUES: [u32; 3] = {
    let mut values = [1, 2, 3];
    values.swap(0, 2);
    values.swap(1, 1);
    values
};

const PAIR: (u32, u32) = {
    let mut first = 4;
    let mut second = 5;
    std::mem::swap(&mut first, &mut second);
    (first, second)
};

fn main() {
    let mut owned = [String::from("first"), String::from("second")];
    owned.swap(0, 0);
    owned.swap(0, 1);
    println!("{:?} {:?} {:?}", VALUES, PAIR, owned);
}
