// An impl's method may write a bound the trait writes with an associated
// type (`F: FnMut(B, Self::Item) -> B`) as the type it is for the block
// (`I::Item`, or the block's own parameter), or as `Self::Item` again.

trait Walk {
    type Item;
    fn step(&mut self) -> Option<Self::Item>;
    fn walk<B, F: FnMut(B, Self::Item) -> B>(mut self, init: B, mut f: F) -> B
    where
        Self: Sized,
    {
        let mut acc = init;
        while let Some(x) = self.step() {
            acc = f(acc, x);
        }
        acc
    }
}

struct Up(u32);

impl Walk for Up {
    type Item = u32;
    fn step(&mut self) -> Option<u32> {
        self.0 += 1;
        if self.0 < 4 { Some(self.0) } else { None }
    }
}

struct Twice<W>(W);

impl<W: Walk> Walk for Twice<W> {
    type Item = W::Item;
    fn step(&mut self) -> Option<W::Item> {
        self.0.step()
    }
    fn walk<B, F: FnMut(B, W::Item) -> B>(self, init: B, f: F) -> B {
        self.0.walk(init, f)
    }
}

struct Same<W>(W);

impl<W: Walk> Walk for Same<W> {
    type Item = W::Item;
    fn step(&mut self) -> Option<W::Item> {
        self.0.step()
    }
    fn walk<B, F: FnMut(B, Self::Item) -> B>(self, init: B, f: F) -> B {
        self.0.walk(init, f)
    }
}

fn main() {
    let _ = Twice(Up(0)).walk(0, |a, x| a + x);
    let _ = Same(Up(0)).walk(0, |a, x| a + x);
}
