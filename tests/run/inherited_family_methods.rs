trait Read<'a>: Sized {
    fn read(data: &'a [u32]) -> Self;
}

trait Rule<'a>: Read<'a> {
    type Input: Copy + 'a;

    fn inputs(&self) -> &'a [Self::Input];

    fn first(&self) -> Self::Input {
        self.inputs()[0]
    }

    fn accepts(&self, value: &mut u32, check: &impl Fn(&mut u32, u32) -> bool) -> bool {
        check(value, self.inputs().len() as u32)
    }
}

trait Chain<'a>: Rule<'a> {
    fn backtrack(&self) -> &'a [Self::Input];
}

struct Record<'a>(&'a [u32]);

impl<'a> Read<'a> for Record<'a> {
    fn read(data: &'a [u32]) -> Self {
        Self(data)
    }
}

impl<'a> Rule<'a> for Record<'a> {
    type Input = u32;
    fn inputs(&self) -> &'a [u32] {
        self.0
    }
}

impl<'a> Chain<'a> for Record<'a> {
    fn backtrack(&self) -> &'a [u32] {
        self.0
    }
}

fn positions<'a, R: Chain<'a>>(rule: R) -> (&'a [R::Input], &'a [R::Input]) {
    (rule.inputs(), rule.backtrack())
}

fn main() {
    let data = [29, 31];
    let rule = Record::read(&data);
    let first = rule.first();
    assert!(rule.accepts(&mut 2, &|glyph, count| *glyph == count));
    let (inputs, backtrack) = positions(rule);
    println!("{} {} {}", first, inputs[0], backtrack[1]);
}
