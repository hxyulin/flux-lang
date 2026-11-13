trait Base { type Node; }
trait Directed: Base { type Neighbors: Iterator<Item = Self::Node>; fn neighbors(&self) -> Self::Neighbors; }
trait Neighbors: Base { type Iter: Iterator<Item = Self::Node>; fn iter(&self) -> Self::Iter; }
struct Undirected<G>(G);
impl<G: Base> Base for Undirected<G> { type Node = G::Node; }
impl<G: Directed> Neighbors for Undirected<G> {
    type Iter = std::iter::Chain<G::Neighbors, G::Neighbors>;
    fn iter(&self) -> Self::Iter { self.0.neighbors().chain(self.0.neighbors()) }
}
struct Graph;
impl Base for Graph { type Node = u8; }
impl Directed for Graph { type Neighbors = std::array::IntoIter<u8, 2>; fn neighbors(&self) -> Self::Neighbors { [3, 5].into_iter() } }
trait Mode {
    type Output<T>;
    fn bind<T, F: FnOnce() -> T>(f: F) -> Self::Output<T>;
    fn combine<T, U, V, F: FnOnce(T, U) -> V>(a: Self::Output<T>, b: Self::Output<U>, f: F) -> Self::Output<V>;
}
struct Emit;
impl Mode for Emit {
    type Output<T> = T;
    fn bind<T, F: FnOnce() -> T>(f: F) -> T { f() }
    fn combine<T, U, V, F: FnOnce(T, U) -> V>(a: T, b: U, f: F) -> V { f(a, b) }
}
struct Check;
impl Mode for Check {
    type Output<T> = ();
    fn bind<T, F: FnOnce() -> T>(_: F) {}
    fn combine<T, U, V, F: FnOnce(T, U) -> V>(_: (), _: (), _: F) {}
}
fn collect<M: Mode>(item: M::Output<u8>) -> M::Output<Vec<u8>> {
    let init = M::bind(|| Vec::new());
    M::combine(init, item, |mut values, item| { values.push(item); values })
}
fn count<M: Mode>() -> M::Output<usize> { M::bind(|| 7) }
fn main() {
    assert_eq!(Undirected(Graph).iter().sum::<u8>(), 16);
    assert_eq!(collect::<Emit>(9), vec![9]);
    collect::<Check>(());
    assert_eq!(count::<Emit>(), 7usize);
    struct NotClone(u8);
    let items = [NotClone(3), NotClone(5), NotClone(7)];
    let mut a = items.windows(2);
    let mut b = a.clone();
    assert_eq!(a.next().unwrap()[0].0, b.next().unwrap()[0].0);
    println!("{:?}", [1u8, 2, 3].windows(2).enumerate());
}
