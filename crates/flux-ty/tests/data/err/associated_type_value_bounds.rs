trait Mask { fn first(self) -> usize; }
trait Vector { type Mask; }
fn missing_bound<V: Vector>(value: V::Mask) {
    let first = V::Mask::first;
    let _ = first(value);
}
fn missing_item<V: Vector>() {
    let _ = <V as Vector>::Mask::NOT_A_CONST;
}
fn main() {}
