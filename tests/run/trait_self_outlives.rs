use std::borrow::Cow;
trait Arg{fn into_text<'b>(self)->Cow<'b,str> where Self:'b;}
impl<'a> Arg for &'a str{fn into_text<'b>(self)->Cow<'b,str>where Self:'b{Cow::Borrowed(self)}}
impl Arg for String{fn into_text<'b>(self)->Cow<'b,str>where Self:'b{Cow::Owned(self)}}
fn convert<'a,A:Arg+'a>(arg:A)->Cow<'a,str>{arg.into_text()}
fn main(){
 let owned=String::from("borrowed");let borrowed=convert(owned.as_str());assert_eq!(&*borrowed,"borrowed");
 let allocated=convert(String::from("owned"));assert_eq!(&*allocated,"owned");
 println!("{} {}",borrowed,allocated);
}
