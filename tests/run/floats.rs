static REFERENCE: &f32 = &-1.25;
const SUM: f32 = (1.5 + 2.25) * 2.0;
const NEG_ZERO: f64 = f64::from_bits(0x8000000000000000);
const NAN_BITS: u32 = f32::from_bits(0x7f800001).to_bits();
const BITS: u32 = f32::from_bits(0x3f800001).to_bits();
const CAST: i8 = 1000.5f64 as i8;
const WIDE: u128 = 1.0e38f64 as u128;
const WIDE_FLOAT: f64 = u128::MAX as f64;
const REM: f32 = -7.0f32 % 3.0;
fn classify_literal(x:f64)->u8 {match x {0.0=>1,1.5=>2,_=>3}}
fn direct(x: f32, y: f64) -> (f32, f64) { (x+SUM, y/2.0) }
extern "C" fn c_float(x: f32, y: f64) -> f64 { x as f64 + y }
unsafe extern "C" { fn fabs(x:f64)->f64; }
fn generic<T: core::ops::Add<Output=T>>(x:T,y:T)->T{x+y}
fn main() {
    assert_eq!(REFERENCE.to_bits(),(-1.25f32).to_bits());
    assert_eq!(NAN_BITS,0x7f800001);
    assert_eq!(BITS, 0x3f800001);
    assert_eq!((u128::MAX as f64).to_bits(), WIDE_FLOAT.to_bits());
    assert_eq!(1.0e38f64 as u128, WIDE);
    assert_eq!(f64::NAN as i128,0);
    assert_eq!(f64::INFINITY as i128,i128::MAX);
    assert_eq!(f64::NEG_INFINITY as i128,i128::MIN);
    assert_eq!(f64::INFINITY as u128,u128::MAX);
    assert_eq!(f64::NEG_INFINITY as u128,0);
    assert_eq!(CAST, 127);
    assert_eq!(REM.to_bits(), (-1.0f32).to_bits());
    assert_eq!(NEG_ZERO.to_bits(), 0x8000000000000000);
    assert_eq!((-NEG_ZERO).to_bits(), 0);
    let pointer:fn(f32,f64)->(f32,f64)=direct;
    let (x,y)=pointer(1.0,9.0);
    assert_eq!(x.to_bits(), 8.5f32.to_bits());
    assert_eq!(y.to_bits(), 4.5f64.to_bits());
    assert_eq!(generic(1.25f64,2.5).to_bits(),3.75f64.to_bits());
    let cp:extern "C" fn(f32,f64)->f64=c_float;
    assert_eq!(cp(1.25,2.5).to_bits(),3.75f64.to_bits());
    let native:unsafe extern "C" fn(f64)->f64=fabs;
    unsafe { assert_eq!(native(-5.25).to_bits(),5.25f64.to_bits()); }
    let nan = f64::from_bits(0x7ff8000000000123);
    assert!(nan.is_nan()); assert!(nan != nan); assert!(!(nan == nan));
    assert!(!(nan < 1.0)); assert!(!(nan >= 1.0));
    assert_eq!(nan as i32,0); assert_eq!(nan as u64,0);
    let negative_zero = -0.0f64;
    assert_eq!(classify_literal(negative_zero),1);
    assert_eq!(classify_literal(nan),3);
    assert!(negative_zero == 0.0); assert!(negative_zero.is_sign_negative());
    assert_eq!((negative_zero % 3.0).to_bits(),NEG_ZERO.to_bits());
    assert_eq!((1.0f64/0.0).to_bits(),f64::INFINITY.to_bits());
    assert_eq!((-1000.5f64) as i8,-128);
    assert_eq!(1000.5f32 as u8,255);
    assert_eq!((-1.5f32) as u8,0);
    assert_eq!(f64::INFINITY as i64,i64::MAX);
    assert_eq!(f64::NEG_INFINITY as u32,0);
    let narrowed=1.000000059604644775390625f64 as f32;
    assert_eq!(narrowed.to_bits(),1.0f32.to_bits());
    assert_eq!((16777217u32 as f32).to_bits(),16777216.0f32.to_bits());
    let mut update=2.5f32; update+=1.25; update*=2.0;
    assert_eq!(update.to_bits(),7.5f32.to_bits());
    update += &0.0f32;
    assert_eq!(f64::from(1.25f32).to_bits(),1.25f64.to_bits());
    let referenced=&update;
    assert_eq!((referenced+1.0f32).to_bits(),8.5f32.to_bits());
    assert_eq!(core::mem::size_of::<f32>(),4); assert_eq!(core::mem::align_of::<f64>(),8);
    println!("floats {} {} {}",x.to_bits(),y.to_bits(),update.to_bits());
}
