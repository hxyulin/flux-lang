trait SimdValue {
    type Element;
    fn from_lanes(lanes: [Self::Element; 4]) -> Self;
}
struct Wide(f64);
impl SimdValue for Wide {
    type Element = f64;
    fn from_lanes(lanes: [f64; 4]) -> Self {
        Self(lanes[0] + lanes[1] + lanes[2] + lanes[3])
    }
}
struct Matrix<T>(T);
impl<T: SimdValue> From<[Matrix<T::Element>; 4]> for Matrix<T> {
    fn from([a, b, c, d]: [Matrix<T::Element>; 4]) -> Self {
        Matrix(T::from_lanes([a.0, b.0, c.0, d.0]))
    }
}

#[derive(Clone, Copy)]
struct Vector;
impl From<Vector> for Matrix<f64> {
    fn from(_: Vector) -> Self {
        Matrix(1.0)
    }
}
fn array<T>(f: impl FnMut(usize) -> T) -> [T; 4] {
    core::array::from_fn(f)
}

fn main() {
    let result = Matrix::<Wide>::from(array(|_| Vector.into()));
    println!("{}", result.0.0);
}
