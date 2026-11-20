struct Pair<T> {
    x: T,
    y: T,
}

trait Vector<T> {
    type Mask;
}

trait Signed<T>: Vector<T> {}

trait Float<T>: Signed<T> {
    fn mask(self) -> Self::Mask;
}

impl<T> Vector<T> for Pair<T> {
    type Mask = Pair<bool>;
}

impl<T> Signed<T> for Pair<T> {}

impl<T> Float<T> for Pair<T> {
    fn mask(self) -> Self::Mask {
        Self::Mask { x: true, y: false }
    }
}

fn update<V: Vector<u8, Mask = Pair<bool>>>(base: V::Mask, y: bool) -> V::Mask {
    V::Mask { y, ..base }
}

fn main() {
    let mask = Pair { x: 2u8, y: 3 }.mask();
    println!("{} {}", mask.x, mask.y);
    let updated = update::<Pair<u8>>(mask, true);
    println!("{} {}", updated.x, updated.y);
}
