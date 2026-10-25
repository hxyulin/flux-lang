use std::cell::Cell;
struct Value<'a> { id: u32, clones: &'a Cell<u32>, drops: &'a Cell<u32> }
impl Clone for Value<'_> {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Value { id: self.id, clones: self.clones, drops: self.drops }
    }
}
impl Drop for Value<'_> { fn drop(&mut self) { self.drops.set(self.drops.get() + 1); } }
fn children() -> impl Iterator<Item=u32> + Clone {
    let inline = [Some(3), None];
    let first: &'static [u32] = &[5, 7];
    let second: &'static [u32] = &[11, 13];
    inline.into_iter().flatten().chain(first.iter().chain(second.iter()).copied())
}
fn main() {
    let clones = Cell::new(0);
    let drops = Cell::new(0);
    {
        let make = |id| Value { id, clones: &clones, drops: &drops };
        let mut original = [make(1),make(2),make(3),make(4),make(5)].into_iter();
        drop(original.next().unwrap()); drop(original.next_back().unwrap());
        let mut copied = original.clone();
        assert_eq!(clones.get(),3);
        assert_eq!(copied.as_slice().len(),3);
        assert_eq!(copied.as_slice()[0].id,2);
        drop(copied.next().unwrap());
        drop(copied);
        assert_eq!(drops.get(),5);
        assert_eq!(original.map(|v|v.id).sum::<u32>(),9);
    }
    assert_eq!(drops.get(),8);
    let mut chain = children();
    assert_eq!(chain.next(),Some(3));
    let copied = chain.clone();
    assert_eq!(chain.sum::<u32>(),36);
    assert_eq!(copied.sum::<u32>(),36);
    let mut zst=[(),(),()].into_iter();zst.next();
    assert_eq!(zst.clone().count(),2);
    let empty: [String;0]=[];
    assert_eq!(empty.into_iter().clone().count(),0);
    println!("array clone {} {}",clones.get(),drops.get());
}
