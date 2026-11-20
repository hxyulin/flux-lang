fn make<const N: usize>() -> [u8; N] {
    [5; _]
}

fn take(values: [u8; 3]) {
    println!("{:?}", values);
}

struct Tracker(&'static str);

impl Drop for Tracker {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let inferred_later = [9; _];
    take(inferred_later);
    take([7; (_)]);
    println!("{:?}", make::<4>());

    let zero: [String; 0] = [String::new(); _];
    let one: [String; 1] = [String::from("once"); _];
    let constant: [String; 2] = [const { String::new() }; _];
    println!("{} {} {}", zero.len(), one[0], constant.len());

    let _inferred: [Tracker; 0] = [Tracker("inferred"); _];
    let _explicit = [Tracker("explicit"); 0];
    let _constant: [Tracker; 0] = [const { Tracker("unused") }; _];
    println!("done");
}
