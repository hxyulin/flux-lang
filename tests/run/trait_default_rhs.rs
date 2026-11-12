use std::ops::Add;
struct W<T>(T);
impl<'a,T> Add<&'a T> for &'a W<T> where &'a T:Add {
 type Output=W<<&'a T as Add>::Output>;
 fn add(self,rhs:&'a T)->Self::Output{W((&self.0).add(rhs))}
}
fn main(){let left=W(5i32);let right=7i32;let sum=&left+&right;assert_eq!(sum.0,12);println!("{}",sum.0);}
