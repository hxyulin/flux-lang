struct Pair {
    left: i32,
    right: i32,
}

impl Pair {
    fn refs(&mut self) -> [&mut i32; 2] {
        [&mut self.left, &mut self.right]
    }

    fn get(&mut self, index: usize) -> &mut i32 {
        self.refs()[index]
    }
}

fn main() {
    let mut pair = Pair { left: 4, right: 9 };
    for index in 0..2 {
        *pair.get(index) += 3;
    }
    println!("{} {}", pair.left, pair.right);
}
