//! Geometry and a zoo with traits and generics, no trait objects: shapes
//! as an enum implementing a `Shape` trait, generic statistics over
//! anything with an area, derived orderings, `Ordering`-based sorting
//! keys, `max_by_key`/`min_by` and a generic `largest`.

use core::cmp::Ordering;

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

/// Areas and perimeters in fixed point: millimetres and square
/// millimetres, with pi approximated as 355/113.
trait Shape {
    fn area(&self) -> i64;
    fn perimeter(&self) -> i64;
    fn name(&self) -> &'static core::ffi::CStr;

    fn compactness(&self) -> i64 {
        // area / perimeter^2, scaled by 10_000.
        let p = self.perimeter();
        if p == 0 { 0 } else { self.area() * 10_000 / (p * p) }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Point {
    x: i64,
    y: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Figure {
    Circle { center: Point, radius: i64 },
    Rect { corner: Point, w: i64, h: i64 },
    Triangle(Point, Point, Point),
}

fn isqrt(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

fn dist(a: Point, b: Point) -> i64 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    isqrt(dx * dx + dy * dy)
}

impl Shape for Figure {
    fn area(&self) -> i64 {
        match *self {
            Figure::Circle { radius, .. } => radius * radius * 355 / 113,
            Figure::Rect { w, h, .. } => w * h,
            Figure::Triangle(a, b, c) => ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2,
        }
    }

    fn perimeter(&self) -> i64 {
        match self {
            Figure::Circle { radius, .. } => 2 * radius * 355 / 113,
            Figure::Rect { w, h, .. } => 2 * (w + h),
            Figure::Triangle(a, b, c) => dist(*a, *b) + dist(*b, *c) + dist(*c, *a),
        }
    }

    fn name(&self) -> &'static core::ffi::CStr {
        match self {
            Figure::Circle { .. } => c"circle",
            Figure::Rect { w, h, .. } if w == h => c"square",
            Figure::Rect { .. } => c"rectangle",
            Figure::Triangle(..) => c"triangle",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Species {
    Cat,
    Dog,
    Parrot,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Animal {
    species: Species,
    age: u32,
    weight_g: u32,
}

trait Describe {
    fn describe(&self);
}

impl Describe for Animal {
    fn describe(&self) {
        let kind = match self.species {
            Species::Cat => c"cat",
            Species::Dog => c"dog",
            Species::Parrot => c"parrot",
        };
        unsafe { printf(c"%s aged %u weighing %ug\n".as_ptr(), kind.as_ptr(), self.age, self.weight_g) };
    }
}

impl Describe for Figure {
    fn describe(&self) {
        unsafe {
            printf(c"%s: area %lld perimeter %lld compactness %lld\n".as_ptr(), self.name().as_ptr(),
                self.area(), self.perimeter(), self.compactness());
        }
    }
}

fn describe_all<T: Describe>(items: &[T]) {
    for item in items {
        item.describe();
    }
}

fn total_area<S: Shape>(shapes: &[S]) -> i64 {
    shapes.iter().map(Shape::area).sum()
}

fn largest<T: PartialOrd + Copy>(items: &[T]) -> Option<T> {
    let mut best = *items.first()?;
    for &x in &items[1..] {
        if x > best {
            best = x;
        }
    }
    Some(best)
}

fn by_species_then_age(a: &Animal, b: &Animal) -> Ordering {
    a.species.cmp(&b.species).then_with(|| b.age.cmp(&a.age))
}

fn main() {
    let figures = [
        Figure::Circle { center: Point { x: 0, y: 0 }, radius: 100 },
        Figure::Rect { corner: Point { x: 5, y: 5 }, w: 120, h: 80 },
        Figure::Rect { corner: Point { x: -5, y: 2 }, w: 90, h: 90 },
        Figure::Triangle(Point { x: 0, y: 0 }, Point { x: 300, y: 0 }, Point { x: 0, y: 400 }),
        Figure::Circle { center: Point { x: 9, y: 9 }, radius: 7 },
    ];
    describe_all(&figures);
    unsafe { printf(c"total area %lld\n".as_ptr(), total_area(&figures)) };

    let biggest = figures.iter().max_by_key(|f| f.area()).unwrap();
    let roundest = figures.iter().max_by(|a, b| a.compactness().cmp(&b.compactness())).unwrap();
    unsafe {
        printf(c"biggest: %s, roundest: %s\n".as_ptr(), biggest.name().as_ptr(), roundest.name().as_ptr());
    }
    let squares = figures.iter().filter(|f| f.name() == c"square").count();
    unsafe { printf(c"%zu square(s)\n".as_ptr(), squares) };

    let corners = [Point { x: 3, y: 1 }, Point { x: -2, y: 8 }, Point { x: 3, y: 0 }, Point { x: 1, y: 1 }];
    let max_point = largest(&corners).unwrap();
    let min_point = corners.iter().copied().min().unwrap();
    unsafe {
        printf(c"max point (%lld, %lld), min point (%lld, %lld)\n".as_ptr(), max_point.x, max_point.y,
            min_point.x, min_point.y);
    }

    let mut zoo = [
        Animal { species: Species::Dog, age: 3, weight_g: 12_000 },
        Animal { species: Species::Cat, age: 7, weight_g: 4_200 },
        Animal { species: Species::Parrot, age: 30, weight_g: 400 },
        Animal { species: Species::Cat, age: 2, weight_g: 3_100 },
        Animal { species: Species::Dog, age: 11, weight_g: 30_500 },
    ];
    zoo.sort_by(by_species_then_age);
    describe_all(&zoo);
    let heaviest = zoo.iter().max_by_key(|a| a.weight_g).unwrap();
    let youngest = zoo.iter().min_by(|a, b| a.age.cmp(&b.age)).unwrap();
    let cats = zoo.iter().filter(|a| a.species == Species::Cat).count();
    let oldest_species = largest(&[Species::Dog, Species::Cat]).unwrap();
    unsafe {
        printf(c"heaviest weighs %u, youngest is %u, %zu cats, max species %d\n".as_ptr(),
            heaviest.weight_g, youngest.age, cats, oldest_species as i32);
    }
    let ord = Point { x: 1, y: 2 }.cmp(&Point { x: 1, y: 3 });
    unsafe { printf(c"ordering %d\n".as_ptr(), ord as i32) };
}
