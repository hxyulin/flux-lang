use std::collections::HashSet;
#[derive(PartialEq)] struct Value(u32);
struct Heterogeneous;
impl PartialEq<u32> for Heterogeneous {fn eq(&self,rhs:&u32)->bool{*rhs==17}}
impl PartialEq<bool> for Heterogeneous {fn eq(&self,rhs:&bool)->bool{*rhs}}
fn main() {
    let class=|n:u32|match n {1=>'b',2=>'h',4=>'s',8=>'d',_=>unreachable!()};
    assert_eq!(class(4),'s');
    let mut value=Value(9);let shared=Value(9);
    assert!(Some(&shared)==Some(&mut value));
    assert!(Heterogeneous==17u32);assert!(Heterogeneous==true);
    let mut values=HashSet::new();values.insert(2u32);values.extend(&[3u32,4,4]);
    assert_eq!(values.len(),3);assert!(values.contains(&3));
    println!("core inference {} {} {}",class(2),value.0,values.len());
}
