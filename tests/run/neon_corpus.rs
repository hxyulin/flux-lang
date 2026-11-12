use std::arch::aarch64::*;
fn main(){unsafe{
 assert!(std::arch::is_aarch64_feature_detected!("neon"));
 let mut checksum=0u64;
 for seed in 0..256u32{
  let mut left=[0u8;16];let mut right=[0u8;16];let mut mask=[0u8;16];
  for i in 0..16{left[i]=(seed+i as u32*17)as u8;right[i]=(255-seed+i as u32*29)as u8;mask[i]=(i*17)as u8;}
  let a:uint8x16_t=std::mem::transmute(left);let b:uint8x16_t=std::mem::transmute(right);let m:uint8x16_t=std::mem::transmute(mask);
  let sa:int8x16_t=std::mem::transmute(a);let sb:int8x16_t=std::mem::transmute(b);
  for vector in [vaddq_u8(a,b),vsubq_u8(a,b),vqsubq_u8(a,b),vbicq_u8(a,b),vcleq_u8(a,b),vcgtq_s8(sa,sb),vmvnq_u8(a),vbslq_u8(m,a,b)]{
   let lanes:[u8;16]=std::mem::transmute(vector);
   for value in lanes{checksum=checksum.wrapping_mul(31).wrapping_add(value as u64);}
  }
  checksum=checksum.wrapping_add(vmaxvq_u8(a)as u64);
 }
 let words:uint16x8_t=std::mem::transmute([0u16,1,32767,32768,65535,42,512,8192]);
 let shifted:[u16;8]=std::mem::transmute(vshlq_n_u16::<15>(words));println!("{:?}",shifted);
 let right:[u16;8]=std::mem::transmute(vshrq_n_u16::<16>(words));assert_eq!(right,[0u16;8]);
 let combined=vorrq_u16(vshlq_n_u16::<3>(words),vshrq_n_u16::<5>(words));
 let bytes:[u8;16]=std::mem::transmute(vreinterpretq_u8_u16(combined));println!("{:?}",bytes);
 let lanes:uint32x4_t=std::mem::transmute([0u32,42,0x80000000,u32::MAX]);
 println!("{} {} {}",checksum,vgetq_lane_u32::<0>(lanes),vgetq_lane_u32::<3>(lanes));
}}
