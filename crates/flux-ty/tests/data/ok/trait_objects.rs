// Trait objects: coercions at each coercion site, calls of their
// methods, upcasting, generic traits, associated types and closures.

trait Named {
    fn id(&self) -> i32;
    fn twice(&self) -> i32 {
        self.id() * 2
    }
}
trait Shape: Named {
    fn area(&self) -> i32;
    fn grow(&mut self, by: i32);
}
struct Sq(i32);
impl Named for Sq {
    fn id(&self) -> i32 {
        1
    }
}
impl Shape for Sq {
    fn area(&self) -> i32 {
        self.0 * self.0
    }
    fn grow(&mut self, by: i32) {
        self.0 += by;
    }
}
trait Conv<T> {
    fn conv(&self, x: T) -> T;
}
impl Conv<i32> for Sq {
    fn conv(&self, x: i32) -> i32 {
        x + self.0
    }
}
struct Holder<'a> {
    shape: &'a dyn Shape,
}

fn up(s: &dyn Shape) -> &dyn Named {
    s
}
fn generic<T: Shape + ?Sized>(x: &T) -> i32 {
    x.area()
}
fn apply(f: &dyn Fn(i32) -> i32) -> i32 {
    f(1)
}
fn sum(it: &mut dyn Iterator<Item = u32>) -> u32 {
    let mut s = 0;
    for x in it {
        s += x;
    }
    s
}

fn main() {
    let mut a = Sq(2);
    let b = Sq(3);
    a.grow(1);
    let shapes: [&dyn Shape; 2] = [&a, &b];
    let h = Holder { shape: &b };
    let o: Option<&dyn Named> = Some(&a);
    let t = (1, &b as &dyn Shape);
    let c: &dyn Conv<i32> = &b;
    let p: *const dyn Shape = &b;
    let k = 4;
    let _ = shapes[0].area() + h.shape.twice() + up(t.1).id() + generic(shapes[1]);
    let _ = c.conv(1) + apply(&|x| x + k) + o.map_or(0, |n| n.id());
    let _ = sum(&mut (0..3u32)) + unsafe { (*p).area() } as u32;
}
