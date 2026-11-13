struct Range<const MIN: i32, const MAX: i32>(i32);
impl<const MIN:i32,const MAX:i32> Range<MIN,MAX> {
    const fn new<const V:i32>() -> Self { Self(V) }
    const fn get(self) -> i32 { self.0 }
}
const fn limit() -> i32 { 60 }
type Whole = Range<{ -limit() }, { limit() }>;
struct Offset(i32);
impl Offset {
    const UTC: Self = Self::from_whole(Whole::new::<0>());
    const fn from_whole(value: Whole) -> Self { Self(value.get()) }
    const fn get(self) -> i32 { self.0 }
}
fn timestamp() -> i32 {
    type Timestamp = Range<{ Offset::UTC.get() }, { Offset::UTC.get() }>;
    Timestamp::new::<0>().get()
}
fn main() { println!("{}",timestamp()); }
