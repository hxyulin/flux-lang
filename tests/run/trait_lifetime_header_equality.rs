struct View<'a, T, I>
where
    I: Iterator<Item = T>,
{
    text: &'a str,
    items: I,
}

trait IntoView<'a, T, I>
where
    I: Iterator<Item = T>,
{
    fn view(self, text: &'a str) -> View<'a, T, I>;
}

impl<'a, T, I> IntoView<'a, T, I> for I
where
    I: Iterator<Item = T>,
{
    fn view(self, text: &'a str) -> View<'a, T, I> {
        View { text, items: self }
    }
}

fn open<'a, T, I>(iter: I, text: &'a str) -> View<'a, T, I>
where
    I: Iterator<Item = T> + IntoView<'a, T, I>,
{
    iter.view(text)
}

fn main() {
    let text = String::from("borrowed");
    let mut view = open([3, 5].into_iter(), &text);
    println!("{} {}", view.text, view.items.next().unwrap());
}
