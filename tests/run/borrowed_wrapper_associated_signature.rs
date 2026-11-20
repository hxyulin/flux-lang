#![allow(type_alias_bounds)]

trait Point {
    type Scalar;
}
impl Point for u32 {
    type Scalar = u32;
}

trait Envelope {
    type Point: Point;
}
impl Envelope for () {
    type Point = u32;
}

trait Object {
    type Envelope: Envelope;
}
impl Object for u32 {
    type Envelope = ();
}

type Distance<T: Query> = <<T::Envelope as Envelope>::Point as Point>::Scalar;
trait Query: Object {
    fn distance(&self, point: &<Self::Envelope as Envelope>::Point) -> Distance<Self>;
}
impl Query for u32 {
    fn distance(&self, point: &u32) -> u32 {
        self.abs_diff(*point)
    }
}

struct Forward<'a, T: Object>(&'a T);
impl<T: Object> Object for Forward<'_, T> {
    type Envelope = T::Envelope;
}
impl<T: Query> Query for Forward<'_, T> {
    fn distance(&self, point: &<Self::Envelope as Envelope>::Point) -> Distance<Self> {
        self.0.distance(point)
    }
}

fn main() {
    let value = 14u32;
    let wrapper = Forward(&value);
    println!("{}", wrapper.distance(&9));
}
