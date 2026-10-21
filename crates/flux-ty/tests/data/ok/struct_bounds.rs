// Bounds on structs, in the parameter list and in `where` clauses: a
// field may name a bound's associated type, and wherever the struct is
// named its bounds must hold, so generic code that names it repeats them.

trait Shape {
    type Out;
    fn area(&self) -> i32;
    fn make(&self) -> Self::Out;
}

trait Solid: Shape {}

struct Square(i32);

impl Shape for Square {
    type Out = i32;
    fn area(&self) -> i32 {
        self.0 * self.0
    }
    fn make(&self) -> i32 {
        self.0
    }
}

impl Solid for Square {}

struct Cached<S: Shape> {
    shape: S,
    made: S::Out,
}

struct Both<A, B>
where
    A: Shape,
    B: Shape<Out = i32>,
{
    a: A,
    b: B,
    sum: B::Out,
}

// A parameter used only through its associated type is used.
struct Made<S: Shape>(S::Out);

// A derive bounds the associated types its fields name too.
#[derive(Clone, Copy, PartialEq)]
struct Pair<S: Shape>(S, S::Out);

impl Clone for Square {
    fn clone(&self) -> Square {
        Square(self.0)
    }
}

impl Copy for Square {}

impl PartialEq for Square {
    fn eq(&self, other: &Square) -> bool {
        self.0 == other.0
    }
}

// A supertrait's bound holds wherever the subtrait's does.
struct Deep<T: Solid>(Cached<T>);

impl<S: Shape> Cached<S> {
    fn new(shape: S) -> Self {
        let made = shape.make();
        Cached { shape, made }
    }
}

impl<S: Shape<Out = i32>> Cached<S> {
    fn twice(&self) -> i32 {
        self.made * 2
    }
}

fn area<T: Shape>(c: &Cached<T>) -> i32 {
    c.shape.area()
}

fn deep<T: Solid>(d: Deep<T>) -> Cached<T> {
    d.0
}

fn main() {
    let c = Cached::new(Square(3));
    let n: i32 = c.made + c.twice() + area(&c);
    let both = Both {
        a: Square(1),
        b: Square(2),
        sum: n,
    };
    let d = deep(Deep(Cached::new(both.b)));
    let _ = (d.made, both.sum, both.a.area());
    let pair = Pair(Square(5), 25);
    let _ = (pair.clone() == pair, Made::<Square>(1).0);
    let explicit: Cached<Square> = Cached::<Square>::new(Square(4));
    let _ = <Cached<Square>>::new(explicit.shape);
}
