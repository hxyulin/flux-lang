fn empty<'a,T>(_:&'a mut [T])->&'a mut[T]{&mut [][..]}
fn partition(data:&mut[u8],flag:bool)->(&mut[u8],&mut[u8]){if flag{data.split_at_mut(1)}else{(&mut [][..],data)}}
struct Owner(u8);
impl Drop for Owner{fn drop(&mut self){assert_eq!(self.0,7);}}
fn main(){
 let mut values=[1u8,2,3];let (none,rest)=partition(&mut values,false);assert!(none.is_empty());rest[0]=9;
 let (head,tail)=partition(&mut values,true);assert_eq!(head,&[9]);assert_eq!(tail,&[2,3]);
 assert!(empty::<Owner>(&mut []).is_empty());
 let zero:&'static mut[u8]=&mut [][..];assert!(zero.is_empty());
 println!("{:?} {}",values,zero.len());
}
