fn main() {
    let tie32=f32::from_bits(0x4a21b1d5);
    let tie64=f64::from_bits(0x43195a754da32e35);
    assert_eq!(format!("{:?}",tie32),"2649205.3");
    assert_eq!(format!("{:?}",-tie32),"-2649205.3");
    assert_eq!(format!("{:?}",tie64),"1784083569494925.3");
    assert_eq!(format!("{:?}",-tie64),"-1784083569494925.3");
    assert_eq!(format!("{:.1?}",tie32),"2649205.2");
    assert_eq!(format!("{:.1?}",tie64),"1784083569494925.2");
    for text in ["0","-0","1.0000000596046447753906250001","1.000000059604644775390625","3.4028234663852885981170418348451692544e38","1e-45","5e-324","2.2250738585072014e-308","1.7976931348623157e308","1e9999","-1e-9999","NaN","-inf","+INFINITY"] {
        let a=text.parse::<f32>().unwrap();let b=text.parse::<f64>().unwrap();
        println!("{} {} {} {:?} {:?}",text,a.to_bits(),b.to_bits(),a,b);
    }
    for bad in [""," ","1 ","0x1p2","1_0","1e","+","nan(1)",".e1"] {
        assert!(bad.parse::<f32>().is_err());assert!(bad.parse::<f64>().is_err());
    }
    for x in [1.234567f64,1e20,1e-10,0.0,-0.0,9.9999,2.5,3.5] {
        println!("{:.2?} {:.0?} {:+012.3?} {:*^20.4?}",x,x,x,x);
    }
    println!("{:+08.1100?}",f64::from_bits(1));
    let mut seed=0x123456789abcdef0u64;
    let mut i=0;
    while i<4096 {
        seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let a=f32::from_bits(seed as u32);let b=f64::from_bits(seed);
        println!("{:?} {:?}",a,b);
        if !a.is_nan() { assert_eq!(format!("{:?}",a).parse::<f32>().unwrap().to_bits(),a.to_bits()); }
        if !b.is_nan() { assert_eq!(format!("{:?}",b).parse::<f64>().unwrap().to_bits(),b.to_bits()); }
        let wide=((seed as u128)<<64)|seed.rotate_left(17) as u128;
        println!("{} {} {} {} {} {}",(wide as f32).to_bits(),(wide as f64).to_bits(),((wide as i128) as f32).to_bits(),((wide as i128) as f64).to_bits(),b as i128,b as u128);
        i+=1;
    }
}
