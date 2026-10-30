trait SliceFamily { type Out<T>: ?Sized; }
struct SliceOwner;
impl SliceFamily for SliceOwner { type Out<T> = [T]; }
trait View {
    type Output: ?Sized;
    fn view(&self) -> &Self::Output;
}
impl View for String {
    type Output = str;
    fn view(&self) -> &str { self.as_str() }
}
fn view_size<T: View>(value: &T) -> usize { core::mem::size_of_val(value.view()) }
fn describe(value: &(impl core::fmt::Debug + ?Sized)) -> usize {
    println!("{:?}", value);
    core::mem::size_of_val(value)
}
mod primitive {
    pub use bool as Flag;
    pub use u32 as Word;
    pub use char;
}
fn main() {
    assert_eq!(describe("hello"),5);
    assert_eq!(view_size(&String::from("projected")),9);
    let family: &<SliceOwner as SliceFamily>::Out<u8> = &[1,2,3];
    assert_eq!(family.len(),3);
    assert_eq!(describe(&[1u32,2,3][..]),12);
    let flag:primitive::Flag=true;
    let number:primitive::Word=primitive::Word::MAX;
    let letter:primitive::char='x';
    println!("{} {} {}",flag,number,letter);
}
