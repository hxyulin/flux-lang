//! DST boxes clone their contents into independent allocations; collection
//! also supports non-Clone elements, empty slices and zero-sized elements.
#[derive(Clone)]
struct Owned(String);

impl Drop for Owned {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

#[derive(Clone)]
struct Zst;
impl Drop for Zst {
    fn drop(&mut self) {
        println!("drop zst");
    }
}

struct MoveOnly(i32);

fn main() {
    let text: Box<str> = Box::from("héλ");
    let copy = text.clone();
    println!(
        "{} {} {} {}",
        text,
        copy,
        text.len(),
        text.as_ptr() as usize != copy.as_ptr() as usize
    );
    let empty: Box<str> = Box::from("");
    println!("empty {}", empty.clone().len());

    let original: Box<[Owned]> = vec![Owned(String::from("a")), Owned(String::from("b"))]
        .into_iter()
        .collect();
    let mut cloned = original.clone();
    cloned[0].0.push('!');
    println!("{} {} {}", original[0].0, cloned[0].0, cloned.len());
    drop(original);
    println!("survives {}", cloned[1].0);
    drop(cloned);

    let empty_slice: Box<[Owned]> = core::iter::empty().collect();
    println!("empty slice {}", empty_slice.clone().len());
    let values: Box<[MoveOnly]> = (1..4).map(MoveOnly).collect();
    println!("move only {} {}", values.len(), values[2].0);
    let zsts: Box<[Zst]> = (0..2).map(|_| Zst).collect();
    println!("zst {}", zsts.clone().len());
    println!("end");
}
