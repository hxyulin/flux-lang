// Borrowing a place also finds trait methods whose Self is a reference.
trait Mark {}
struct Value(u8);
impl Mark for Value {}
struct Borrowed(u8);
trait Pick {
    fn pick(self) -> u8;
}
impl<T: Mark> Pick for T {
    fn pick(self) -> u8 {
        11
    }
}
impl Pick for &Value {
    fn pick(self) -> u8 {
        self.0
    }
}
impl Pick for &Borrowed {
    fn pick(self) -> u8 {
        self.0
    }
}
impl Pick for &mut Borrowed {
    fn pick(self) -> u8 {
        88
    }
}
struct Inherent;
impl Inherent {
    fn pick(&self) -> u8 {
        41
    }
}
impl Pick for &Inherent {
    fn pick(self) -> u8 {
        42
    }
}
trait Total {
    fn total(self) -> u32;
}
impl Total for &[u8] {
    fn total(self) -> u32 {
        let mut sum = 0;
        for x in self {
            sum += *x as u32;
        }
        sum
    }
}
trait View {
    type Output;
    fn view(self) -> Self::Output;
}
impl<'a> View for &'a Borrowed {
    type Output = &'a u8;
    fn view(self) -> &'a u8 {
        &self.0
    }
}
trait Edit {
    type Output;
    fn edit(self) -> Self::Output;
}
impl<'a> Edit for &'a mut Borrowed {
    type Output = &'a mut u8;
    fn edit(self) -> &'a mut u8 {
        &mut self.0
    }
}

fn convert(s: &[u8]) {
    let a: [u8; 4] = s[..4].try_into().unwrap();
    let b: [u8; 2] = s[1..3].try_into().unwrap();
    let empty: [u8; 0] = s[..0].try_into().unwrap();
    let wrong: Result<[u8; 3], _> = s[..4].try_into();
    println!("{:?} {:?} {:?} {}", a, b, empty, wrong.is_err());
}
fn main() {
    convert(&[1, 3, 5, 7, 9]);
    println!("{} {}", Value(99).pick(), Borrowed(23).pick());
    let mut b = Borrowed(31);
    println!(
        "{} {} {} {}",
        b.view(),
        b.pick(),
        Inherent.pick(),
        [2u8, 4, 8].total()
    );
    *b.edit() += 1;
    println!("{}", b.view());
}
