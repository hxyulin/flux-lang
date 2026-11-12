use std::collections::{HashMap,HashSet,VecDeque};
use std::cell::RefCell;
const NUMBER:u64=unsafe{std::hint::assert_unchecked(true);Some(7u64).unwrap_unchecked().unchecked_mul(6)};
fn typed_entry<S:std::hash::BuildHasher>(map:&mut HashMap<u32,u32,S>)->std::collections::hash_map::Entry<'_,u32,u32>{map.entry(7)}
fn main(){
 let mut entries=HashMap::with_hasher(std::hash::BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default());
 println!("{:?}",typed_entry(&mut entries));typed_entry(&mut entries).or_insert(9);
 println!("{:?}",typed_entry(&mut entries));
 println!("{:?} {:?}",entries.iter(),entries.clone().into_iter());println!("{:?}",entries.iter_mut());println!("{:?}",entries.drain());assert!(entries.is_empty());

 assert!([1,2,2,4].into_iter().is_sorted());assert!(![1,3,2].into_iter().is_sorted());
 assert!(![0.0f32,f32::NAN].into_iter().is_sorted());assert!([3i32,2,1].into_iter().is_sorted_by_key(|n|-n));
 let mut calls=0;assert!(![3,2,1].into_iter().is_sorted_by(|a,b|{calls+=1;a<=b}));assert_eq!(calls,1);

 let mut values=vec![1,2,3,4];let bytes:&mut[i32]=values.as_mut();bytes[1]=8;
 {let mut drain=values.drain(1..3);println!("{:?}",drain);assert_eq!(drain.next(),Some(8));println!("{:?}",drain);}
 assert_eq!(values,vec![1,4]);
 let mut deque:VecDeque<i32>=(0..8).collect();for _ in 0..3{deque.pop_front();}deque.extend(8..11);
 deque.retain_mut(|n|{*n+=10;*n%2==0});assert_eq!(deque.into_iter().collect::<Vec<_>>(),vec![14,16,18,20]);
 let mut map=HashMap::new();for i in 0..30{map.insert(i,i*2);}for i in 0..15{map.remove(&i);}
 map.try_reserve(100).unwrap();assert!(map.capacity()>=115);assert!(map.try_reserve(usize::MAX).is_err());
 assert_eq!(map.len(),15);assert_eq!(map.get(&29),Some(&58));
 let formatted=RefCell::new(17);println!("{} {:?}",formatted.borrow(),formatted.borrow());println!("{}",formatted.borrow_mut());
 let a:HashSet<i32>=[1,2,3].into_iter().collect();let b:HashSet<i32>=[2,4].into_iter().collect();
 let mut union:Vec<_>=(&a|&b).into_iter().collect();union.sort();assert_eq!(union,vec![1,2,3,4]);
 assert_eq!((&a & &b).into_iter().collect::<Vec<_>>(),vec![2]);
 assert_eq!((&a - &b).len(),2);assert_eq!((&a ^ &b).len(),3);
 assert!(RefCell::new(vec![1,2])<RefCell::new(vec![1,3]));
 assert_eq!(unsafe{20u8.unchecked_add(22)},42);assert_eq!(unsafe{20i32.unchecked_sub(22)},-2);
 assert_eq!(unsafe{2u32.unchecked_shl(4)},32);assert_eq!(unsafe{(-8i64).unchecked_shr(2)},-2);
 let duration=std::time::Duration::new(7,123456789);assert_eq!(duration.subsec_micros(),123456);assert_eq!(duration.subsec_millis(),123);
 println!("{} {} {}",NUMBER,map.len(),union.len());
}
