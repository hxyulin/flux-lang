fn main() {
    for value in [0.0f64,-0.0,1.0,-1.0,f64::MAX,f64::MIN_POSITIVE,f64::from_bits(1),f64::INFINITY,f64::NEG_INFINITY,f64::NAN,f64::from_bits(0x43195a754da32e35)] {
        println!("{}|{:+035}|{:.0}|{:.2}|{:.20}|{:*^40.3}",value,value,value,value,value,value);
    }
    let mut seed=0xa0e451cafe675432u64;
    for _ in 0..1024 {
        seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;
        let a=f64::from_bits(seed);let b=f32::from_bits(seed as u32);
        println!("{}|{:+024.4}|{}|{:.8}",a,a,b,b);
        if a.is_finite(){let text=a.to_string();let parsed:f64=text.parse().unwrap();assert_eq!(parsed.to_bits(),a.to_bits());}
    }
}
