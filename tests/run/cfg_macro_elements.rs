macro_rules! enabled_expr{($meta:meta,$value:expr)=>{#[$meta]{$value}}}
macro_rules! nested{($meta:meta,$value:expr)=>{enabled_expr!($meta,$value)}}
fn sum(a:i32,b:i32)->i32{a+b}
fn main(){
    let mut effects=0;
    let a=[nested!(cfg(any()),missing_name()),nested!(cfg(all()),{effects+=1;7}),9];
    assert_eq!(a,[7,9]);assert_eq!(effects,1);
    let tuple=(nested!(cfg(any()),undefined),nested!(cfg_attr(all(),cfg(any())),undefined),11,13);
    assert_eq!(tuple,(11,13));
    let value=sum(nested!(cfg(any()),undefined),nested!(cfg(all()),17),19);
    assert_eq!(value,36);
    println!("{:?} {:?} {} {}",a,tuple,value,effects);
}
