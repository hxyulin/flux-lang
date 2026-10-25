use std::slice::{from_mut, from_ref};
fn main() {
    let initial=17u32; let shared=from_ref(&initial); assert_eq!(shared,[17]);
    let mut answer=19u32;from_mut(&mut answer)[0]+=4;assert_eq!(answer,23);
    let text=String::from("one");
    let view=from_ref(&text);
    assert_eq!(view.len(),1);assert_eq!(view[0],"one");
    assert!(std::ptr::eq(view.as_ptr(), &text));
    let mut value=29u32;
    let view=from_mut(&mut value);
    view[0]+=2;assert_eq!(value,31);
    let unit=();assert_eq!(from_ref(&unit).len(),1);
    let mut unit=();assert_eq!(from_mut(&mut unit).len(),1);
    println!("single value {} {} {}",shared[0],answer,value);
}
