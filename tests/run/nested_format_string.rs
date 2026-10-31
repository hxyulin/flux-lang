macro_rules! text {()=>{concat!("number ","{}",stringify!(u8))};}
fn main(){let x=format!(text!(),17); println!("{x}"); println!(concat!("nested ","{}"),23); println!(concat!("{value}"), value=31);}
