use std::cmp::Ordering;

#[derive(PartialEq, PartialOrd)]
struct Mark(u16);
struct Glyph(u32);

impl PartialEq<Glyph> for Mark {
    fn eq(&self, glyph: &Glyph) -> bool {
        u32::from(self.0) == glyph.0
    }
}
impl PartialEq<Mark> for Glyph {
    fn eq(&self, mark: &Mark) -> bool {
        self.0 == u32::from(mark.0)
    }
}
impl PartialOrd<Glyph> for Mark {
    fn partial_cmp(&self, glyph: &Glyph) -> Option<Ordering> {
        u32::from(self.0).partial_cmp(&glyph.0)
    }
}
impl PartialOrd<Mark> for Glyph {
    fn partial_cmp(&self, mark: &Mark) -> Option<Ordering> {
        self.0.partial_cmp(&u32::from(mark.0))
    }
}

fn main() {
    let glyph = Glyph(17);
    assert!((Mark(1)..=Mark(17)).contains(&glyph));
    assert!(!(Mark(1)..Mark(17)).contains(&glyph));
    assert!((Mark(17)..).contains(&glyph));
    assert!((..=Mark(17)).contains(&glyph));
    assert!(!(..Mark(17)).contains(&glyph));

    let mut singleton = 17..=17;
    assert!(singleton.contains(&17));
    assert_eq!(singleton.next(), Some(17));
    assert!(!singleton.contains(&17));

    struct Element(u16);
    let data = [Element(1), Element(2), Element(3), Element(4), Element(5)];
    let mut chunks = data.chunks_exact(2);
    assert_eq!(chunks.next().unwrap()[0].0, 1);
    let mut copy = chunks.clone();
    assert_eq!(copy.next().unwrap()[0].0, 3);
    assert!(copy.next().is_none());
    assert_eq!(chunks.next().unwrap()[0].0, 3);
    assert_eq!(copy.remainder()[0].0, 5);
    println!("range and chunks");
}
