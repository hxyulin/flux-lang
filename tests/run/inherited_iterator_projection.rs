trait Component<'a> {
    fn name(&self) -> &'a str;
}

impl<'a> Component<'a> for &'a str {
    fn name(&self) -> &'a str {
        *self
    }
}

trait Components<'a>:
    Iterator<Item = Self::Component> + DoubleEndedIterator<Item = Self::Component>
{
    type Component: Component<'a>;
}

impl<'a> Components<'a> for std::iter::Once<&'a str> {
    type Component = &'a str;
}

trait Encoding {
    type Parts<'a>: Components<'a>;
    fn parts<'a>(text: &'a str) -> Self::Parts<'a>;
}

struct Plain;
impl Encoding for Plain {
    type Parts<'a> = std::iter::Once<&'a str>;
    fn parts<'a>(text: &'a str) -> Self::Parts<'a> {
        std::iter::once(text)
    }
}

fn first<'a, E: Encoding>(text: &'a str) -> &'a str {
    E::parts(text).next_back().unwrap().name()
}

fn first_from_parts<'a, C: Components<'a>>(mut parts: C) -> &'a str {
    parts.next_back().unwrap().name()
}

fn main() {
    let text = String::from("borrowed component");
    println!(
        "{} {}",
        first::<Plain>(&text),
        first_from_parts(Plain::parts(&text))
    );
}
