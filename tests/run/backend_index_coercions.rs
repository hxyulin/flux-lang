use std::collections::HashMap;
use std::ops::{Index,IndexMut};
struct MutableOnly(u32);
impl Index<&mut u32> for MutableOnly {
    type Output=u32;
    fn index(&self,key:&mut u32)->&u32 { *key+=1; &self.0 }
}
struct Writable(u64);
impl Index<&u32> for Writable {type Output=u64;fn index(&self,_:&u32)->&u64{&self.0}}
impl IndexMut<&u32> for Writable {fn index_mut(&mut self,_:&u32)->&mut u64{&mut self.0}}
fn main() {
    let mut map=HashMap::new();map.insert(3u32,17u64);
    let mut key=3;let borrowed=&mut key;
    assert_eq!(map[borrowed],17);
    assert_eq!(*borrowed,3);
    let both=MutableOnly(29);assert_eq!(both[borrowed],29);
    assert_eq!(*borrowed,4);
    *borrowed=3;let mut writable=Writable(0);writable[borrowed]=31;
    assert_eq!(writable.0,31);
    println!("index {} {}",key,map[&key]);
}
