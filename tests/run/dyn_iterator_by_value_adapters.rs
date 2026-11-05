// By-value adapters on `&mut dyn Iterator` and `Box<dyn Iterator>` take
// `Self` through `impl Iterator for &mut I` (or `Box<I>`), and structs
// that store `I::Item` must lay it out as the trait object's `Item`.

trait Source {
    type Out;
    fn get(&self) -> Self::Out;
}

impl Source for u8 {
    type Out = i64;
    fn get(&self) -> i64 {
        i64::from(*self) * 3
    }
}

impl<T: Source + ?Sized> Source for &mut T {
    type Out = T::Out;
    fn get(&self) -> T::Out {
        (**self).get()
    }
}

struct Held<S: Source> {
    source: S,
    cached: Option<S::Out>,
}

fn hold<S: Source>(source: S) -> Held<S> {
    Held {
        source,
        cached: None,
    }
}

fn first_peeked<I: ?Sized + Iterator<Item = i32>>(iter: &mut I) -> Option<i32> {
    let mut p = iter.peekable();
    p.peek().copied()
}

fn main() {
    let mut it = vec![1, 2, 3].into_iter();
    let r: &mut dyn Iterator<Item = i32> = &mut it;
    let mut p = r.peekable();
    println!("peek {:?}", p.peek());
    println!("peek again {:?}", p.peek());
    println!("next {:?}", p.next());
    println!("rest {:?}", p.collect::<Vec<_>>());

    let mut it = vec![1, 2, 3, 4].into_iter();
    let mut r: &mut dyn Iterator<Item = i32> = &mut it;
    println!("map {:?}", r.map(|x| x * 10).next());
    println!("take {:?}", r.take(1).collect::<Vec<_>>());
    println!("by_ref {:?}", (&mut r).by_ref().next());
    println!("left {:?}", it.next());

    let mut it = vec![7, 8].into_iter();
    let r: &mut dyn Iterator<Item = i32> = &mut it;
    println!("explicit {:?}", (&mut *r).peekable().peek());

    let mut it = vec![9, 10].into_iter();
    println!("generic sized {:?}", first_peeked(&mut it));
    let r: &mut dyn Iterator<Item = i32> = &mut it;
    println!("generic dyn {:?}", first_peeked(r));

    let mut b: Box<dyn Iterator<Item = i32>> = Box::new(vec![11, 12, 13].into_iter());
    println!("box by_ref {:?}", b.by_ref().peekable().peek());
    let mut p = b.peekable();
    println!("box peek {:?}", p.peek());
    println!("box rest {:?}", p.collect::<Vec<_>>());

    let mut x = 5u8;
    let s: &mut dyn Source<Out = i64> = &mut x;
    let mut held = hold(s);
    println!("held empty {}", held.cached.is_none());
    held.cached = Some(held.source.get());
    println!("held {:?}", held.cached);
    println!("size {}", std::mem::size_of::<Held<&mut dyn Source<Out = i64>>>());
}
