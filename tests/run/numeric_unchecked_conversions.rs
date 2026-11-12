fn main(){
    unsafe {
        println!("{} {} {} {}",(-12.75f32).to_int_unchecked::<i8>(),255.75f32.to_int_unchecked::<u8>(),65535.0f64.to_int_unchecked::<u16>(),(-32768.0f64).to_int_unchecked::<i16>());
        println!("{} {} {}",(-0.99f32).to_int_unchecked::<u32>(),(-2147483648.0f64).to_int_unchecked::<i32>(),4294967295.0f64.to_int_unchecked::<u32>());
        println!("{} {}",(-9223372036854775808.0f64).to_int_unchecked::<i64>(),9223372036854775808.0f64.to_int_unchecked::<u64>());
        println!("{} {}",1267650600228229401496703205376.0f64.to_int_unchecked::<u128>(),(-1267650600228229401496703205376.0f64).to_int_unchecked::<i128>());
        println!("{} {}",31.9f32.to_int_unchecked::<usize>(),(-31.9f32).to_int_unchecked::<isize>());
    }
}
