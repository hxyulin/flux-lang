trait Path {
    type Commands: Iterator<Item = u32>;

    fn commands(&self) -> Self::Commands;
}

struct Outline(Vec<u32>);
struct View<'a>(&'a [u32]);

impl<'a> Path for View<'a> {
    type Commands = std::iter::Copied<std::slice::Iter<'a, u32>>;

    fn commands(&self) -> Self::Commands {
        self.0.iter().copied()
    }
}

impl Outline {
    fn path(&self) -> impl Path + '_ {
        View(&self.0)
    }
}

fn main() {
    let outline = Outline(vec![1, 2, 3]);
    let mut sum = 0;
    for command in outline.path().commands() {
        sum += command;
    }
    assert_eq!(sum, 6);
    println!("{sum}");
}
