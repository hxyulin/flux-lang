//! Std's inherent float math. Core keeps representation and comparisons.
unsafe extern "C" {
    fn sqrtf(x:f32)->f32;
    fn sqrt(x:f64)->f64;
}
// The system math library, which rustc's std also calls for these.
mod libm {
    unsafe extern "C" {
        pub fn powf(x:f32,y:f32)->f32; pub fn pow(x:f64,y:f64)->f64;
        pub fn expf(x:f32)->f32; pub fn exp(x:f64)->f64;
        pub fn exp2f(x:f32)->f32; pub fn exp2(x:f64)->f64;
        pub fn logf(x:f32)->f32; pub fn log(x:f64)->f64;
        pub fn log2f(x:f32)->f32; pub fn log2(x:f64)->f64;
        pub fn log10f(x:f32)->f32; pub fn log10(x:f64)->f64;
        pub fn fdimf(x:f32,y:f32)->f32; pub fn fdim(x:f64,y:f64)->f64;
        pub fn cbrtf(x:f32)->f32; pub fn cbrt(x:f64)->f64;
        pub fn hypotf(x:f32,y:f32)->f32; pub fn hypot(x:f64,y:f64)->f64;
        pub fn sinf(x:f32)->f32; pub fn sin(x:f64)->f64;
        pub fn cosf(x:f32)->f32; pub fn cos(x:f64)->f64;
        pub fn tanf(x:f32)->f32; pub fn tan(x:f64)->f64;
        pub fn asinf(x:f32)->f32; pub fn asin(x:f64)->f64;
        pub fn acosf(x:f32)->f32; pub fn acos(x:f64)->f64;
        pub fn atanf(x:f32)->f32; pub fn atan(x:f64)->f64;
        pub fn atan2f(x:f32,y:f32)->f32; pub fn atan2(x:f64,y:f64)->f64;
        pub fn expm1f(x:f32)->f32; pub fn expm1(x:f64)->f64;
        pub fn log1pf(x:f32)->f32; pub fn log1p(x:f64)->f64;
        pub fn sinhf(x:f32)->f32; pub fn sinh(x:f64)->f64;
        pub fn coshf(x:f32)->f32; pub fn cosh(x:f64)->f64;
        pub fn tanhf(x:f32)->f32; pub fn tanh(x:f64)->f64;
        pub fn asinhf(x:f32)->f32; pub fn asinh(x:f64)->f64;
        pub fn acoshf(x:f32)->f32; pub fn acosh(x:f64)->f64;
    }
}
#[rustc_intrinsic]
const fn fmaf32(x:f32,y:f32,z:f32)->f32 {loop {}}
#[rustc_intrinsic]
const fn fmaf64(x:f64,y:f64,z:f64)->f64 {loop {}}

/// Integral rounding from IEEE bits; no ambient rounding mode is consulted.
/// Modes: truncate, ceiling, floor, nearest with ties to even, nearest with
/// ties away from zero.
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
        let up=match mode {1=>!negative,2=>negative,3=>magnitude>half,4=>magnitude>=half,_=>false};
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
        4=>remainder>=half,
        _=>false,
    };
    if up {truncated+unit} else {truncated}
}
macro_rules! unary {
    ($($name:ident $c:ident),*) => {$(
        pub fn $name(self)->Self {
            // SAFETY: the C function accepts every IEEE input and has no
            // other preconditions; the C ABI matches the Rust float type.
            unsafe {libm::$c(self)}
        }
    )*};
}
macro_rules! binary {
    ($($name:ident $c:ident),*) => {$(
        pub fn $name(self,other:Self)->Self {
            // SAFETY: the C function accepts every IEEE input and has no
            // other preconditions; the C ABI matches the Rust float type.
            unsafe {libm::$c(self,other)}
        }
    )*};
}
macro_rules! math {
    ($t:ty,$u:ty,$single:expr,$sqrt:ident,$fma:ident,[$($un:ident $uc:ident),*],[$($bin:ident $bc:ident),*]) => {
        impl $t {
            unary!($($un $uc),*);
            binary!($($bin $bc),*);
            /// Rounds half-way cases away from zero.
            pub const fn round(self)->Self {
                if self.is_nan() {return self+self;}
                Self::from_bits(integral(self.to_bits() as u64,$single,4) as $u)
            }
            pub const fn fract(self)->Self {self-self.trunc()}
            pub fn div_euclid(self,rhs:Self)->Self {
                let q=(self/rhs).trunc();
                if self%rhs<0.0 {return if rhs>0.0 {q-1.0} else {q+1.0};}
                q
            }
            pub fn rem_euclid(self,rhs:Self)->Self {
                let r=self%rhs;
                if r<0.0 {r+rhs.abs()} else {r}
            }
            pub fn log(self,base:Self)->Self {self.ln()/base.ln()}
            pub fn sin_cos(self)->(Self,Self) {(self.sin(),self.cos())}
            pub fn atanh(self)->Self {0.5*((2.0*self)/(1.0-self)).ln_1p()}
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
math!(f32,u32,true,sqrtf,fmaf32,
    [exp expf,exp2 exp2f,ln logf,log2 log2f,log10 log10f,cbrt cbrtf,sin sinf,cos cosf,tan tanf,asin asinf,acos acosf,atan atanf,
     exp_m1 expm1f,ln_1p log1pf,sinh sinhf,cosh coshf,tanh tanhf,asinh asinhf,acosh acoshf],
    [powf powf,abs_sub fdimf,hypot hypotf,atan2 atan2f]);
math!(f64,u64,false,sqrt,fmaf64,
    [exp exp,exp2 exp2,ln log,log2 log2,log10 log10,cbrt cbrt,sin sin,cos cos,tan tan,asin asin,acos acos,atan atan,
     exp_m1 expm1,ln_1p log1p,sinh sinh,cosh cosh,tanh tanh,asinh asinh,acosh acosh],
    [powf pow,abs_sub fdim,hypot hypot,atan2 atan2]);
