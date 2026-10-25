const MIN32:f32=3.0f32.min(f32::NAN);
const MAX64:f64=f64::NAN.max(7.0);
const CEIL:f32=(-0.25f32).ceil();
const FLOOR:f64=(-0.25f64).floor();
const TRUNC:f32=(-1.75f32).trunc();
const EVEN:f64=2.5f64.round_ties_even();
const FUSED32:f32=(1.0f32+f32::EPSILON).mul_add(1.0-f32::EPSILON,-1.0);
const FUSED64:f64=f64::MAX.mul_add(2.0,-f64::MAX);
fn bits32(x:f32)->u32{if x.is_nan(){u32::MAX}else if x==0.0{0}else{x.to_bits()}}
fn bits64(x:f64)->u64{if x.is_nan(){u64::MAX}else if x==0.0{0}else{x.to_bits()}}
fn powers(x:f64,n:i32)->f64{x.powi(n)}
fn power32(x:f32,n:i32)->f32{x.powi(n)}
fn main(){
    assert_eq!(MIN32,3.0);assert_eq!(MAX64,7.0);
    assert_eq!(CEIL.to_bits(),(-0.0f32).to_bits());assert_eq!(FLOOR,-1.0);
    assert_eq!(TRUNC,-1.0);assert_eq!(EVEN,2.0);
    assert_eq!(FUSED32.to_bits(),(-f32::EPSILON*f32::EPSILON).to_bits());
    assert_eq!(FUSED64.to_bits(),f64::MAX.to_bits());
    assert!(f32::NAN.to_bits() & (1u32<<22)!=0);assert!(f64::NAN.to_bits() & (1u64<<51)!=0);
    let fused:fn(f64,f64,f64)->f64=f64::mul_add;
    assert_eq!(fused(f64::MAX,2.0,-f64::MAX),f64::MAX);
    let root:fn(f64)->f64=f64::sqrt;assert_eq!(root(4.0),2.0);
    assert_eq!((-0.0f64).sqrt().to_bits(),(-0.0f64).to_bits());assert!((-1.0f32).sqrt().is_nan());
    let inputs=[0.0f64,-0.0,0.25,-0.25,0.5,-0.5,1.5,-1.5,2.5,-2.5,3.5,-3.5,f64::INFINITY,f64::NEG_INFINITY,f64::NAN,f64::from_bits(0x7ff0000000000001),f64::from_bits(1),-f64::from_bits(1)];
    for x in inputs {
        println!("{} {} {} {} {}",bits64(x.min(0.0)),bits64(x.max(0.0)),bits64(x.sqrt()),bits64(x.mul_add(1.25,-0.5)),bits64(x.powi(-3)));
        if !x.is_nan() {println!("{} {} {} {}",x.ceil().to_bits(),x.floor().to_bits(),x.trunc().to_bits(),x.round_ties_even().to_bits());}
        assert_eq!(bits64(x.min(f64::NAN)),bits64(x));assert_eq!(bits64(f64::NAN.max(x)),bits64(x));
        let y=x as f32;
        println!("{} {} {} {} {}",bits32(y.min(0.0)),bits32(y.max(0.0)),bits32(y.sqrt()),bits32(y.mul_add(1.25,-0.5)),bits32(y.powi(-3)));
        if !y.is_nan() {println!("{} {} {} {}",y.ceil().to_bits(),y.floor().to_bits(),y.trunc().to_bits(),y.round_ties_even().to_bits());}
    }
    for n in [i32::MIN,-1074,-1023,-150,-149,-128,-3,-2,-1,0,1,2,3,127,128,1023,1024,i32::MAX] {
        for x in [-2.0f64,-1.0,-0.0,0.0,1.0,2.0,f64::INFINITY,f64::NAN] {
            println!("{} {}",bits64(powers(x,n)),bits32(power32(x as f32,n)));
        }
    }
    let mut seed=0x23456789abcdef01u64;let mut i=0;
    while i<1024 {
        seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let x=f64::from_bits(seed);let y=f32::from_bits(seed as u32);
        println!("{} {} {} {} {} {}",bits64(x.ceil()),bits64(x.floor()),bits64(x.trunc()),bits64(x.round_ties_even()),bits64(x.sqrt()),bits64(x.mul_add(1.25,-0.5)));
        println!("{} {} {} {} {} {}",bits32(y.ceil()),bits32(y.floor()),bits32(y.trunc()),bits32(y.round_ties_even()),bits32(y.sqrt()),bits32(y.mul_add(1.25,-0.5)));
        i+=1;
    }
}
