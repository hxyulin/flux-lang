trait Point: PartialEq {}

trait Envelope {
    type Point;
}

trait Object {
    type Envelope: Envelope;
}

trait Distance: Object {
    fn same(&self, point: &<Self::Envelope as Envelope>::Point) -> bool;
}

struct Aabb<T>(T);

impl<T: Point> Envelope for Aabb<T> {
    type Point = T;
}

impl<T: Point> Object for T {
    type Envelope = Aabb<T>;
}

impl<T: Point> Distance for T {
    fn same(&self, point: &<Self::Envelope as Envelope>::Point) -> bool {
        self == point
    }
}

impl Point for u32 {}

mod scalar {
    trait One {
        fn one() -> Self;
    }

    impl One for u8 {
        fn one() -> Self {
            1
        }
    }

    trait Point {
        type Scalar: One;
    }

    trait Envelope {
        type Point: Point;

        fn one(&self) -> <Self::Point as Point>::Scalar;
    }

    struct Aabb<P>(P);

    impl Point for u32 {
        type Scalar = u8;
    }

    impl<P: Point> Envelope for Aabb<P> {
        type Point = P;

        fn one(&self) -> <Self::Point as Point>::Scalar {
            <Self::Point as Point>::Scalar::one()
        }
    }

    pub fn check() {
        assert_eq!(Aabb(5u32).one(), 1u8);
    }
}

fn main() {
    scalar::check();
    assert!(42u32.same(&42));
    assert!(!42u32.same(&71));
    println!("{}", 42u32.same(&42));
}
