trait Mark<'a> {}
impl<'a> Mark<'a> for () {}

trait Visitor<'a> {
    fn visit(&mut self, input: &'a u32);
}

impl<'a> Visitor<'a> for u32 {
    fn visit(&mut self, input: &'a u32) {
        *self += *input;
    }
}

struct Dispatch<'a> {
    callback: fn(&mut dyn Visitor<'a>),
}

fn visit<'a, T: Mark<'a>>(visitor: &mut dyn Visitor<'a>) {
    static INPUT: u32 = 7;
    visitor.visit(&INPUT);
}

fn dispatch<'a, T: Mark<'a>>() -> Dispatch<'a> {
    Dispatch { callback: visit::<T> }
}

fn main() {
    let mut value = 20;
    let dispatch = dispatch::<()>();
    (dispatch.callback)(&mut value);
    (dispatch.callback)(&mut value);
    println!("{value}");
}
