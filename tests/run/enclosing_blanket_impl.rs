use std::collections::HashSet;
trait Collect {
    fn collect_values<'a>(self, seed: &'a u32) -> HashSet<&'a u32>;
    fn collect_owned(self, seed: &u32) -> HashSet<u32>;
}
impl<'i, I, T> Collect for T
where
    T: IntoIterator<Item = &'i I>,
    I: 'i + Measured,
{
    fn collect_values<'a>(self, seed: &'a u32) -> HashSet<&'a u32> {
        self.into_iter()
            .fold(Default::default(), |mut state, item| {
                state.extend(item.measure(seed));
                state
            })
    }

    fn collect_owned(self, seed: &u32) -> HashSet<u32> {
        self.collect_values(seed).into_iter().copied().collect()
    }
}
trait Measured {
    fn measure<'a>(&self, seed: &'a u32) -> HashSet<&'a u32>;
}
impl Measured for u32 {
    fn measure<'a>(&self, seed: &'a u32) -> HashSet<&'a u32> {
        if *self == 2 {
            [seed].into_iter().collect()
        } else {
            Default::default()
        }
    }
}
fn main() {
    check_concrete_impl();
    let values = [2, 3];
    let seed = 4;
    println!("{}", values.iter().collect_owned(&seed).contains(&4));
}

struct Wrapper<'a>(&'a u32);
trait Visit {
    fn visit(&self) -> u32;
}
impl Visit for Wrapper<'_> {
    fn visit(&self) -> u32 {
        if *self.0 == 0 {
            let item = 3;
            Wrapper(&item).visit()
        } else {
            *self.0
        }
    }
}
fn check_concrete_impl() {
    assert_eq!(Wrapper(&0).visit(), 3);
}
