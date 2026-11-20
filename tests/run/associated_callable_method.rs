use std::ops::Range;

struct Line<S>(S);

trait Segment {
    type Scalar;

    fn each(&self, callback: &mut dyn FnMut(&Line<Self::Scalar>, Range<Self::Scalar>));
}

impl<S: Copy> Segment for Line<S> {
    type Scalar = S;

    fn each(&self, callback: &mut dyn FnMut(&Line<S>, Range<S>)) {
        callback(self, self.0..self.0);
    }
}

fn main() {
    let line = Line(17u32);
    line.each(&mut |point, range| println!("{} {}", point.0, range.start));
}
