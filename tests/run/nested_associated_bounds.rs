trait Endian {
    fn read(self) -> u32;
}
impl Endian for u32 {
    fn read(self) -> u32 {
        self
    }
}
trait Section {
    type Endian: Endian;
    fn endian(self) -> Self::Endian;
}
trait Segment {
    type Endian: Endian;
    type Section: Section<Endian = Self::Endian>;
    fn section(self) -> Self::Section;
}
trait Header {
    type Endian: Endian;
    type Segment: Segment<Endian = Self::Endian, Section = Self::Section>;
    type Section: Section<Endian = Self::Endian>;
    fn segment(self) -> Self::Segment;
}
struct Head<T>(T);
struct Seg<T>(T);
struct Sec<T>(T);
impl<T: Endian> Section for Sec<T> {
    type Endian = T;
    fn endian(self) -> T {
        self.0
    }
}
impl<T: Endian> Segment for Seg<T> {
    type Endian = T;
    type Section = Sec<T>;
    fn section(self) -> Sec<T> {
        Sec(self.0)
    }
}
impl<T: Endian> Header for Head<T> {
    type Endian = T;
    type Segment = Seg<T>;
    type Section = Sec<T>;
    fn segment(self) -> Seg<T> {
        Seg(self.0)
    }
}
trait Value {
    type Out;
}
struct Source<T>(T);
impl<T> Value for Source<T> {
    type Out = T;
}
trait Selected {
    type Out;
}
impl<T> Selected for Source<T> {
    type Out = <Self as Value>::Out;
}
trait Container<T> {
    type Member: Selected<Out = T>;
}
impl<T> Container<T> for Source<T> {
    type Member = Self;
}
fn read<H: Header>(head: H) -> u32 {
    head.segment().section().endian().read()
}
fn main() {
    assert_eq!(read(Head(42u32)), 42);
    let _: Option<<Source<u32> as Container<u32>>::Member> = None;
    println!("nested associated {}", read(Head(57u32)));
}
