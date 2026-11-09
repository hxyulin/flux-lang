trait Get {
    type Item;
    fn get(self) -> Self::Item;
}

impl<'a> Get for &'a u32 {
    type Item = &'a u32;
    fn get(self) -> Self::Item {
        self
    }
}

fn borrow<T>(value: &T) -> &u32
where
    for<'a> &'a T: Get<Item = &'a u32>,
{
    value.get()
}

fn paired<L, R>(left: &L, right: &R) -> u32
where
    for<'a> &'a L: IntoIterator<Item = &'a u32>,
    for<'a> &'a R: IntoIterator<Item = <&'a L as IntoIterator>::Item>,
{
    left.into_iter().chain(right).copied().sum()
}

trait View<'a> {
    type Item;
    fn view(&'a self) -> Self::Item;
}

impl<'a> View<'a> for u32 {
    type Item = &'a u32;
    fn view(&'a self) -> Self::Item {
        self
    }
}

fn opaque() -> impl for<'a> View<'a, Item = &'a u32> {
    42u32
}

enum Choice<L, R> {
    Left(L),
    Right(R),
}

impl<L, R> Choice<L, R> {
    fn iter_mut(&mut self) -> Choice<<&mut L as IntoIterator>::IntoIter, <&mut R as IntoIterator>::IntoIter>
    where
        for<'a> &'a mut L: IntoIterator,
        for<'a> &'a mut R: IntoIterator<Item = <&'a mut L as IntoIterator>::Item>,
    {
        match self {
            Choice::Left(left) => Choice::Left(left.into_iter()),
            Choice::Right(right) => Choice::Right(right.into_iter()),
        }
    }
}

fn main() {
    let value = 7u32;
    let left = vec![1u32, 2];
    let right = vec![3u32, 4];
    let hidden = opaque();
    let boxes = vec![vec![1u32, 2].into_boxed_slice(), vec![3u32].into_boxed_slice()];
    let rows = std::iter::once(&boxes[0]).chain(boxes[1..].iter());
    assert_eq!(rows.flatten().copied().sum::<u32>(), 6);
    let mut choice: Choice<Vec<u32>, Vec<u32>> = Choice::Left(vec![5, 6]);
    match choice.iter_mut() {
        Choice::Left(iter) => { for value in iter { *value += 1; } }
        Choice::Right(iter) => { for value in iter { *value += 1; } }
    }
    match choice {
        Choice::Left(values) => assert_eq!(values, vec![6, 7]),
        Choice::Right(_) => panic!("wrong variant"),
    }
    println!("{} {} {}", borrow(&value), paired(&left, &right), hidden.view());
}
