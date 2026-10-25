//! Std's inherent float math. Core keeps representation and comparisons.
unsafe extern "C" {
    fn sqrtf(x:f32)->f32;
    fn sqrt(x:f64)->f64;
}
#[rustc_intrinsic]
const fn fmaf32(x:f32,y:f32,z:f32)->f32 {loop {}}
#[rustc_intrinsic]
const fn fmaf64(x:f64,y:f64,z:f64)->f64 {loop {}}

/// Integral rounding from IEEE bits; no ambient rounding mode is consulted.
/// Modes: truncate, ceiling, floor, nearest with ties to even.
const fn integral(bits:u64,single:bool,mode:u8)->u64 {
    let fraction_bits=if single {23u32} else {52};
    let sign=if single {1u64<<31} else {1u64<<63};
    let bias=if single {127i32} else {1023};
    let magnitude=bits&!sign;
    if magnitude==0 {return bits;}
    let exponent=(magnitude>>fraction_bits) as i32-bias;
    if exponent>=fraction_bits as i32 {return bits;}
    let negative=bits&sign!=0;
    if exponent<0 {
        let half=((bias-1) as u64)<<fraction_bits;
        let one=(bias as u64)<<fraction_bits;
        let up=match mode {1=>!negative,2=>negative,3=>magnitude>half,_=>false};
        return (bits&sign)|if up {one} else {0};
    }
    let shift=fraction_bits-exponent as u32;
    let unit=1u64<<shift;
    let remainder=bits&(unit-1);
    let truncated=bits&!(unit-1);
    if remainder==0 {return bits;}
    let half=unit>>1;
    let up=match mode {
        1=>!negative,2=>negative,
        3=>remainder>half || remainder==half && truncated&unit!=0,
        _=>false,
    };
    if up {truncated+unit} else {truncated}
}
macro_rules! math {
    ($t:ty,$u:ty,$single:expr,$sqrt:ident,$fma:ident) => {
        impl $t {
            pub const fn trunc(self)->Self {
                if self.is_nan() {return self+self;}
                Self::from_bits(integral(self.to_bits() as u64,$single,0) as $u)
            }
            pub const fn ceil(self)->Self {
                if self.is_nan() {return self+self;}
                Self::from_bits(integral(self.to_bits() as u64,$single,1) as $u)
            }
            pub const fn floor(self)->Self {
                if self.is_nan() {return self+self;}
                Self::from_bits(integral(self.to_bits() as u64,$single,2) as $u)
            }
            pub const fn round_ties_even(self)->Self {
                if self.is_nan() {return self+self;}
                Self::from_bits(integral(self.to_bits() as u64,$single,3) as $u)
            }
            /// One fused operation, rounded once at the target precision.
            pub const fn mul_add(self,a:Self,b:Self)->Self {$fma(self,a,b)}
            pub fn sqrt(self)->Self {
                // SAFETY: sqrt accepts every IEEE input; the C ABI matches $t.
                unsafe {$sqrt(self)}
            }
            /// Exponentiation by squaring with an unsigned exponent magnitude,
            /// including i32::MIN. Precision follows Rust's powi contract.
            pub fn powi(self,n:i32)->Self {
                let mut power=if n<0 {0u32.wrapping_sub(n as u32)} else {n as u32};
                let mut base=self;
                let mut result=1.0;
                while power!=0 {
                    if power&1!=0 {result*=base;}
                    power>>=1;
                    if power!=0 {base*=base;}
                }
                if n<0 {1.0/result} else {result}
            }
        }
    };
}
math!(f32,u32,true,sqrtf,fmaf32);
math!(f64,u64,false,sqrt,fmaf64);
