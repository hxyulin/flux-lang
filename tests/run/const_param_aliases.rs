mod declaration{
 type Count=u16;
 pub struct Stamp<const N:Count=300>;
 impl<const N:Count> Stamp<N>{pub fn value(&self)->Count{N}}
 pub fn value<const N:Count>()->Count{N}
 pub trait Pick<const N:Count>{fn pick()->Count{N}}
 impl<const N:Count> Pick<N> for (){}
}
mod caller{
 type Count=bool;
 pub fn run()->u16{let _:Count=true;super::declaration::value::<500>()}
}
type Flag=bool;
type Letter=char;
type Identity<T>=T;
fn flag<const B:Flag>()->bool{B}
fn letter<const C:Letter>()->char{C}
fn byte<const N:Identity<u8>>()->u8{N}
fn main(){
 let stamp:declaration::Stamp=declaration::Stamp;
 assert_eq!(stamp.value(),300u16);assert_eq!(caller::run(),500u16);
 assert_eq!(<() as declaration::Pick<700>>::pick(),700u16);
 assert!(flag::<true>());assert_eq!(letter::<'λ'>(),'λ');assert_eq!(byte::<255>(),255u8);
 println!("{} {} {}",stamp.value(),caller::run(),byte::<255>());
}
