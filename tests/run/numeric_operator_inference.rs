use std::ops::{Add,Div,Mul,Shl,Sub};
#[derive(Clone,Copy,Debug,Default,PartialEq)]struct N(i64);
impl Add for N{type Output=N;fn add(self,r:N)->N{N(self.0+r.0)}}
impl Sub for N{type Output=N;fn sub(self,r:N)->N{N(self.0-r.0)}}
impl Mul for N{type Output=N;fn mul(self,r:N)->N{N(self.0*r.0)}}
impl Div<u32> for N{type Output=N;fn div(self,r:u32)->N{N(self.0/r as i64)}}
macro_rules! shifts{($($t:ty),*)=>{$(impl Shl<$t> for N{type Output=N;fn shl(self,r:$t)->N{N(self.0<<r)}})*};}
shifts!(i32,u32,u64);
impl Mul<i32> for N{type Output=N;fn mul(self,r:i32)->N{N(self.0*r as i64)}}
impl Mul<u32> for N{type Output=N;fn mul(self,r:u32)->N{N(self.0*r as i64)}}
fn root_step(s:N,q:N)->N{let t=(s<<1)+q;t/3u32}
fn chained(a:N,b:N,c:N)->N{((a+b)*2-c)*((a+b)*2-c)}
fn main(){
 assert_eq!(root_step(N(8),N(5)),N(7));
 assert_eq!(chained(N(2),N(3),N(1)),N(81));
 let a=Default::default();let b=a+1;let _:i32=a;assert_eq!(b,1);
 let left=Default::default();let right=Default::default();let sum=left+right;let _:N=left;let _:N=right;assert_eq!(sum,N(0));
 let input=Default::default();let same=input==1;let _:i32=input;assert!(!same);
 let flag=Default::default();let masked=flag&true;let _:bool=flag;assert!(!masked);
 let value=Default::default();let shifted=value<<1;let _:u8=value;assert_eq!(shifted,0u8);
 println!("{:?} {:?} {}",root_step(N(8),N(5)),chained(N(2),N(3),N(1)),b);
}
