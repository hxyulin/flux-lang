fn value(x:f32)->u8 {
    match x {0.0=>1,-0.0=>2,1.0|2.0=>3,_=>4}
}
fn nested(x:(f64,bool))->u8 {
    match x {(0.0,_)=>1,(_,true)=>2,(_,false)=>3}
}
fn reference(x:&f64)->u8 {match x {1.5=>1,_=>2}}
fn alternative(x:f64)->u8 {let 0.0=x else{return 2;};1}
fn main() {
    assert_eq!(value(0.0),1);assert_eq!(value(-0.0),1);
    assert_eq!(value(1.0),3);assert_eq!(value(2.0),3);
    assert_eq!(value(f32::NAN),4);assert_eq!(value(f32::INFINITY),4);
    assert_eq!(nested((-0.0,false)),1);
    assert_eq!(nested((f64::NAN,true)),2);assert_eq!(nested((f64::NAN,false)),3);
    assert_eq!(reference(&1.5),1);assert_eq!(reference(&f64::NAN),2);
    assert_eq!(alternative(0.0),1);assert_eq!(alternative(-0.0),1);assert_eq!(alternative(f64::NAN),2);
    let x=if let 1.5=1.5f64 {7} else {9};assert_eq!(x,7);
    println!("patterns {} {} {}",value(-0.0),nested((f64::NAN,true)),alternative(f64::NAN));
}
