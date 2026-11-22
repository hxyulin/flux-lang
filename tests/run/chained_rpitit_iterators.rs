trait Visit<T> {
    fn visit_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut T>
    where T: 'a;
}
struct Other(Vec<i32>);
impl Visit<i32> for Vec<i32> {
    fn visit_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut i32> where i32: 'a {
        self.iter_mut()
    }
}
impl Visit<i32> for Other {
    fn visit_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut i32> where i32: 'a {
        self.0.iter_mut()
    }
}
fn visit(value: &mut (Vec<i32>, Other)) {
    std::iter::once(value).flat_map(|x| {
        let iter = std::iter::empty();
        let iter = iter.chain(std::iter::once(&mut x.0).flat_map(|x| Visit::<i32>::visit_mut(x)));
        let iter = iter.chain(std::iter::once(&mut x.1).flat_map(|x| Visit::<i32>::visit_mut(x)));
        iter
    }).for_each(|x| *x += 1);
}
fn main() {
    let mut x = (vec![1], Other(vec![2]));
    visit(&mut x);
    println!("{:?}, {:?}", x.0, x.1.0);
}
