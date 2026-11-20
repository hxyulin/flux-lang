trait Base {
    type Id;
}

impl<G: Base> Base for &G {
    type Id = G::Id;
}

trait Visit: Base {
    type Value;

    fn visit(self, id: Self::Id) -> Self::Value;
}

struct Graph<T>(T);

impl<T> Base for Graph<T> {
    type Id = u32;
}

impl<'a, T> Visit for &'a Graph<T> {
    type Value = &'a T;

    fn visit(self, _: Self::Id) -> Self::Value {
        &self.0
    }
}

struct Wrapped<T>(Graph<T>);

impl<T> Base for Wrapped<T> {
    type Id = u32;
}

impl<'a, T> Visit for &'a Wrapped<T> {
    type Value = <&'a Graph<T> as Visit>::Value;

    fn visit(self, id: Self::Id) -> Self::Value {
        self.0.visit(id)
    }
}

fn main() {
    let graph = Wrapped(Graph(7));
    println!("{}", graph.visit(0));
}
