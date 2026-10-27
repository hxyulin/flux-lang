use std::num::{Saturating, Wrapping};
fn main() {
    assert_eq!((Saturating(u8::MAX)+Saturating(1)).0,u8::MAX);
    assert_eq!((Saturating(i128::MAX)*Saturating(2)).0,i128::MAX);
    assert_eq!((Saturating(i128::MIN)-Saturating(1)).0,i128::MIN);
    assert_eq!((Saturating(i64::MIN)/Saturating(-1)).0,i64::MAX);
    assert_eq!((Saturating(-17i32)%Saturating(5)).0,-2);
    let primitive:core::primitive::i32=7;assert_eq!(primitive,7);
    assert_eq!(f32::from_bits(1).classify(),std::num::FpCategory::Subnormal);
    assert_eq!(f64::NAN.classify(),std::num::FpCategory::Nan);
    assert_eq!((-0.0f64).classify(),std::num::FpCategory::Zero);
    let mut v=Saturating(250u8);v+=Saturating(20);v-=Saturating(7);
    assert_eq!(v.0,248);
    assert_eq!(Saturating(16u8).pow(3).0,255);
    assert_eq!((Wrapping(u8::MAX)+Wrapping(2)).0,1);
    assert_eq!((Wrapping(i128::MAX)*Wrapping(2)).0,-2);
    assert_eq!((Wrapping(i32::MIN)/Wrapping(-1)).0,i32::MIN);
    assert_eq!((Wrapping(i32::MIN)%Wrapping(-1)).0,0);
    assert_eq!((-Wrapping(i32::MIN)).0,i32::MIN);
    assert_eq!(std::f32::EPSILON.to_bits(),f32::EPSILON.to_bits());
    assert!(std::f32::NAN.is_nan());
    assert_eq!(std::f64::MIN.to_bits(),f64::MIN.to_bits());
    println!("saturating {:?} {} {:?}",v,Saturating(i128::MAX),std::f64::EPSILON);
}
