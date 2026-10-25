// Required-method trait owner lifetimes survive generic calls and callbacks.
#[derive(Clone, Copy)]
struct View<'a>(&'a i32);

trait Node<'a>: Copy {
    fn cast(value: View<'a>) -> Option<Self>;
    fn view(self) -> View<'a>;
}

impl<'a> Node<'a> for View<'a> {
    fn cast(value: View<'a>) -> Option<Self> {
        if *value.0 >= 0 { Some(value) } else { None }
    }

    fn view(self) -> View<'a> {
        self
    }
}

fn cast<'a, N: Node<'a>>(values: &[View<'a>]) -> Option<N> {
    values.iter().copied().find_map(N::cast)
}

fn stored<'a, N: Node<'a>>(node: &N) -> &'a i32 {
    node.view().0
}

fn main() {
    let negative = -3;
    let value = 42;
    let values = [View(&negative), View(&value)];
    let result = cast::<View<'_>>(&values).unwrap();
    // The result refers to stored data, independently of this receiver borrow.
    let reference = {
        let temporary = result;
        stored(&temporary)
    };
    println!("{} {}", *reference, *result.view().0);
    println!("{}", cast::<View<'_>>(&values[..1]).is_none());
}
