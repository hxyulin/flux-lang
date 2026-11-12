use std::path::{Path,PathBuf};
const EMPTY:PathBuf=PathBuf::new();
struct Selected;
trait Choice<R=Self>{fn choose(&self,value:&R)->u8;}
impl Choice for Selected{fn choose(&self,_:&Selected)->u8{1}}
impl Choice<&Selected>for Selected{fn choose(&self,_:&&Selected)->u8{2}}
fn main(){
 assert_eq!(<Selected as Choice>::choose(&Selected,&&Selected),1);

 assert!(EMPTY.as_os_str().is_empty());
 let p=Path::new("a");assert!(<Path as PartialEq>::eq(p,&p));
 assert_eq!(<Path as PartialOrd>::partial_cmp(p,&p),Some(std::cmp::Ordering::Equal));
 let ancestors=p.ancestors();let copied=ancestors;let _=ancestors;let _=copied;

 for original in ["/",".","..","x","x.tar.gz/","x/.","x/../",".hidden","x."] {
  for extension in ["","gz","more.txt"] {
   let mut value=PathBuf::from(original);
   let changed=value.add_extension(extension);
   println!("{original:?} {extension:?} {changed} {:?}",value.as_os_str());
   assert_eq!(value,Path::new(original).with_added_extension(extension));
  }
 }
 let mut value=PathBuf::from("ABC/DEf");
 value.as_mut_os_str().make_ascii_lowercase();
 assert_eq!(value,Path::new("abc/def"));
 value.reserve_exact(40);assert!(value.capacity()>=value.as_os_str().len()+40);
 value.try_reserve(3).unwrap();value.try_reserve_exact(4).unwrap();value.shrink_to(5);
 assert!(value.try_reserve(usize::MAX).is_err());
 let leaked=value.leak();leaked.as_mut_os_str().make_ascii_uppercase();println!("{}",leaked.display());
 for input in ["/","//a//b/","///a/./b/","a/../b/","./a/./b",".","/a//../b/."] {
  let absolute=std::path::absolute(input).unwrap();
  let cwd=std::env::current_dir().unwrap();
  println!("{input:?} {:?}",absolute.strip_prefix(&cwd).unwrap_or(&absolute).as_os_str());
 }
 assert_eq!(std::path::absolute("").unwrap_err().kind(),std::io::ErrorKind::InvalidInput);
}
