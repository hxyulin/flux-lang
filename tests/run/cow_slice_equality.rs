use std::borrow::Cow;
#[derive(Clone)]struct Left(u8);
struct Right(u8);
impl PartialEq<Right> for Left{fn eq(&self,other:&Right)->bool{self.0==other.0}}
fn main(){
 let values=[1u8,2,3];let borrowed:Cow<'_,[u8]>=Cow::Borrowed(&values);
 let owned:Cow<'_,[u8]>=Cow::Owned(vec![1,2,3]);
 assert!(borrowed==&values[..]);assert!(owned==vec![1,2,3]);
 let mut rhs=[1u8,2,3];assert!(owned==&mut rhs[..]);
 assert!(borrowed!=&[1u8,2][..]);
 let left=[Left(7)];let cow:Cow<'_,[Left]>=Cow::Borrowed(&left);
 assert!(cow==&[Right(7)][..]);assert!(cow==vec![Right(7)]);
 println!("{} {}",borrowed.len(),owned.len());
}
