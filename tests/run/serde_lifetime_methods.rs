trait Decode<'de>: Sized { fn decode<D: Reader<'de>>(d: D) -> Result<Self,D::Error>; }
trait Reader<'de>: Sized {type Error; fn read<V: Visit<'de>>(self,v:V)->Result<V::Value,Self::Error>;}
trait Visit<'de>: Sized {type Value; fn visit(self, input:&'de str)->Self::Value;}
struct Borrowed<'a>(&'a str);
impl<'de> Visit<'de> for () {type Value=Borrowed<'de>;fn visit(self,input:&'de str)->Self::Value {Borrowed(input)}}
impl<'de> Reader<'de> for &'de str {type Error=();fn read<V:Visit<'de>>(self,v:V)->Result<V::Value,Self::Error>{Ok(v.visit(self))}}
impl<'de> Decode<'de> for Borrowed<'de>{fn decode<D:Reader<'de>>(d:D)->Result<Self,D::Error>{d.read(())}}
fn main(){let text=String::from("borrowed");let b=Borrowed::decode(text.as_str()).unwrap();println!("{}",b.0);}
