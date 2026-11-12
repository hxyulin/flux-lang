struct Wrapper<I>{it:I}
impl<I:Iterator<Item=char>> Wrapper<I>{fn new<T:IntoIterator<IntoIter=I>>(input:T)->Self{Self{it:input.into_iter()}}}
fn main(){
 let text=Wrapper::new("abλ".chars()).it.collect::<String>();assert_eq!(text,"abλ");
 let letters=Wrapper::new(['a','b','c']).it.collect::<String>();assert_eq!(letters,"abc");
 println!("{} {}",text,letters);
}
