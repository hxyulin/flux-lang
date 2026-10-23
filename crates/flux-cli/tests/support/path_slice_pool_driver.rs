use slice::{SliceId, SlicePool};
use std::cell::Cell;

struct Owned<'a> {
    text: String,
    drops: &'a Cell<usize>,
}
impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn owned<'a>(drops: &'a Cell<usize>, text: &str) -> Owned<'a> {
    Owned {
        text: String::from(text),
        drops,
    }
}
fn main() {
    let drops = Cell::new(0);
    {
        let mut pool = SlicePool::default();
        let empty = pool.alloc([]);
        assert!(empty.is_empty());
        assert!(pool[empty].is_empty());
        let first = pool.alloc([owned(&drops, "first"), owned(&drops, "second")]);
        let copy = first;
        let cloned = first.clone();
        assert!(copy == cloned);
        assert_eq!(first.len(), 2);
        assert_eq!(std::mem::size_of_val(&first), 8);
        let default: SliceId<Owned<'_>> = SliceId::default();
        assert!(pool[default].is_empty());
        let mut i = 0;
        while i < 1000 {
            pool.alloc([owned(&drops, "growth")]);
            i += 1;
        }
        assert_eq!(pool[copy][0].text, "first");
        assert_eq!(pool[cloned][1].text, "second");
        let mut scratch = Vec::with_capacity(8);
        scratch.push(owned(&drops, "parent"));
        let start = scratch.len();
        scratch.extend([owned(&drops, "nested-a"), owned(&drops, "nested-b")]);
        let nested = pool.alloc(scratch.drain(start..));
        assert_eq!(scratch.len(), 1);
        scratch.push(owned(&drops, "tail"));
        let parent = pool.alloc(scratch.drain(..));
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), 8);
        assert_eq!(pool[nested][0].text, "nested-a");
        assert_eq!(pool[nested][1].text, "nested-b");
        assert_eq!(pool[parent][0].text, "parent");
        assert_eq!(pool[parent][1].text, "tail");
        assert_eq!(drops.get(), 0);
        println!(
            "slice-path {} {} {}",
            first.len(),
            nested.len(),
            std::mem::size_of_val(&first)
        );
    }
    assert_eq!(drops.get(), 1006);
    println!("drops {}", drops.get());
}
