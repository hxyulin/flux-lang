fn main(){
 assert_eq!(char::UNICODE_VERSION,std::char::UNICODE_VERSION);
 let mut total=0u64;
 for byte in 0..=255u8{
  let escaped=byte.escape_ascii();
  let forward:Vec<u8>=escaped.clone().collect();
  let mut backward:Vec<u8>=escaped.clone().rev().collect();backward.reverse();assert_eq!(forward,backward);
  let text=escaped.to_string();assert_eq!(text.as_bytes(),forward.as_slice());
  for x in forward{total=total.wrapping_mul(31).wrapping_add(x as u64);}
 }
 println!("{} {:?}",total,char::UNICODE_VERSION);
}
