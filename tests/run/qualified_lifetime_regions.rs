#[derive(Clone)]
struct Content<'de> {
    value: &'de u32,
}

struct Owned<'de> {
    iter: <Vec<Content<'de>> as IntoIterator>::IntoIter,
}

struct Borrowed<'a, 'de> {
    iter: <&'a [Content<'de>] as IntoIterator>::IntoIter,
}

trait Take {
    fn take(self) -> Self;
}

impl<'a, T> Take for &'a T {
    fn take(self) -> Self {
        self
    }
}

fn borrowed<'a, 'de>(items: &'a [Content<'de>]) -> Borrowed<'a, 'de> {
    Borrowed {
        iter: items.into_iter(),
    }
}

fn qualified<'a, T>(value: &'a T) -> &'a T {
    <&'a T>::take(value)
}

fn main() {
    let values = [7, 11, 13];
    let contents = values
        .iter()
        .map(|value| Content { value })
        .collect::<Vec<_>>();
    let mut shared = borrowed(&contents);
    assert_eq!(*shared.iter.next().unwrap().value, 7);
    assert_eq!(*shared.iter.next_back().unwrap().value, 13);
    let mut owned = Owned {
        iter: contents.clone().into_iter(),
    };
    assert_eq!(*owned.iter.next().unwrap().value, 7);
    assert_eq!(*owned.iter.next_back().unwrap().value, 13);
    let result = qualified(&values[1]);
    assert!(std::ptr::eq(result, &values[1]));
    println!("qualified {} {}", result, owned.iter.next().unwrap().value);
}
