trait Half { type Double: Double<Half = Self>; }
trait Double { type Half: Half<Double = Self>; fn join(a: Self::Half) -> Self; }
impl Half for u8 { type Double = u16; }
impl Double for u16 { type Half = u8; fn join(a:u8)->Self {a as u16} }
fn widen<T:Half>(x:T)->T::Double where T::Double:Double { T::Double::join(x) }
fn main(){println!("{}",widen(7u8));}
