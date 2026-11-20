trait ParallelIterator {
    type Item;
    fn read(self) -> Self::Item;
}
trait IntoParallelIterator {
    type Item;
    type Iter: ParallelIterator<Item = Self::Item>;
    fn into_par_iter(self) -> Self::Iter;
}
trait IntoParallelRefIterator<'data> {
    type Item;
    type Iter: ParallelIterator<Item = Self::Item>;
    fn par_iter(&'data self) -> Self::Iter;
}
impl<'data, I: 'data + ?Sized> IntoParallelRefIterator<'data> for I
where
    &'data I: IntoParallelIterator,
{
    type Iter = <&'data I as IntoParallelIterator>::Iter;
    type Item = <&'data I as IntoParallelIterator>::Item;
    fn par_iter(&'data self) -> Self::Iter {
        self.into_par_iter()
    }
}

impl ParallelIterator for u32 {
    type Item = u32;
    fn read(self) -> u32 {
        self
    }
}

impl IntoParallelIterator for &u32 {
    type Item = u32;
    type Iter = u32;
    fn into_par_iter(self) -> u32 {
        *self
    }
}

fn main() {
    println!("{}", 42u32.par_iter().read());
}
