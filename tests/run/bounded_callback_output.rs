trait Repetition {
    type Ident;
}
struct Repeat<T>(T);
impl<T> Repetition for &Repeat<T> {
    type Ident = T;
}
enum Component<T, R: Repetition<Ident = T>> {
    Repeat(R),
}
trait Style {
    type TrackList<'a>: Iterator
    where
        Self: 'a;
    fn tracks(&self) -> Self::TrackList<'_>;
}
struct Grid<T>(Vec<Repeat<T>>);
impl<T> Style for Grid<T> {
    type TrackList<'a>
        = std::iter::Map<
        std::slice::Iter<'a, Repeat<T>>,
        fn(&'a Repeat<T>) -> Component<T, &'a Repeat<T>>,
    >
    where
        Self: 'a;

    fn tracks(&self) -> Self::TrackList<'_> {
        self.0.iter().map(|repeat| Component::Repeat(repeat))
    }
}
fn main() {
    let grid = Grid(vec![Repeat(42)]);
    let Some(Component::Repeat(repeat)) = grid.tracks().next() else {
        panic!()
    };
    println!("{}", repeat.0);
}
