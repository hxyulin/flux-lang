fn apply<T>(step:fn(T)->T,value:T)->T {step(value)}
fn identity<T>(value:T)->T {value}
fn nested_call<T>(callback:fn(fn(T)->T,T)->T,step:fn(T)->T,value:T)->T {callback(step,value)}
struct Borrow<'a>(&'a i32);
struct AutoWrapper<B,T>(B,T);
unsafe impl<T:Send> Send for AutoWrapper<Borrow<'_>,T> {}
unsafe impl<T:Sync> Sync for AutoWrapper<Borrow<'_>,T> {}
fn require_auto<T:Send+Sync>(value:T)->T {value}
use core::ptr::NonNull;
const DATA:[u32;2]=[4,9];
const SECOND:u32=unsafe {*DATA.as_ptr().byte_add(4)};
const FIRST:&[u8;2]=match b"abcd".first_chunk::<2>() {Some(value)=>value,None=>panic!()};
const LAST:&[u8;2]=match b"abcd".last_chunk::<2>() {Some(value)=>value,None=>panic!()};
fn library_helpers() {
    let mut result:Result<String,String>=Ok(String::from("ok"));
    assert_eq!(result.as_ref().unwrap(),"ok");
    result.as_mut().unwrap().push('!');
    assert_eq!(result.unwrap(),"ok!");
    let mut result:Result<String,String>=Err(String::from("error"));
    assert_eq!(result.as_ref().unwrap_err(),"error");
    result.as_mut().unwrap_err().push('!');
    assert_eq!(result.unwrap_err(),"error!");
    let mut text=String::from("aé🙂z");
    text.replace_range(1..3,"XY");
    assert_eq!(text,"aXY🙂z");
    text.replace_range(..=0,"front");
    assert_eq!(text,"frontXY🙂z");
    let end=text.len();
    text.replace_range(end..,"!");
    assert_eq!(text,"frontXY🙂z!");
    text.replace_range(..,"");
    assert!(text.is_empty());
    text.replace_range(..,"new");
    assert_eq!(text,"new");
    let controls=(0..=0x100u32).filter_map(char::from_u32).filter(|ch|ch.is_control()).count();
    assert_eq!(controls,65);
    for ch in ['\0','\u{1f}','\u{7f}','\u{85}','\u{9f}'] {assert!(ch.is_control());}
    for ch in [' ','~','\u{a0}','é','🙂'] {assert!(!ch.is_control());}
    let path=std::env::current_dir().unwrap();
    let text=path.to_str().unwrap().to_owned();
    let path=std::path::PathBuf::from(&text);
    assert!(path.is_dir());
    assert!(!path.join("flux-missing-subdirectory-check").is_dir());
    let set=std::collections::HashSet::from([String::from("a"),String::from("b"),String::from("a")]);
    assert_eq!(set.len(),2);
    assert!(set.contains("a"));
    assert!(set.contains("b"));
}
fn main() {
    library_helpers();
    assert_eq!(nested_call(apply::<u32>,|value|value+1,4),5);
    let number=27;
    let wrapper=require_auto(AutoWrapper(Borrow(&number),String::from("borrowed")));
    assert_eq!(*wrapper.0.0,27);
    assert_eq!(wrapper.1,"borrowed");
    assert!(!core::mem::needs_drop::<[u8]>());
    assert!(core::mem::needs_drop::<[String]>());
    assert!(core::mem::needs_drop::<dyn core::fmt::Debug>());
    assert!(!core::mem::needs_drop::<core::mem::ManuallyDrop<[String]>>());
    let mut wrapped=core::mem::ManuallyDrop::new([3u8,7]);
    let unsized_wrapper:&mut core::mem::ManuallyDrop<[u8]>=&mut wrapped;
    unsized_wrapper[1]=9;
    let view:&[u8]=&**unsized_wrapper;
    assert_eq!(view,&[3,9]);
    let nonzero=core::num::NonZeroUsize::new(4).unwrap();
    assert_eq!((nonzero|2usize).get(),6);
    assert_eq!((nonzero|core::num::NonZeroUsize::new(8).unwrap()).get(),12);
    assert_eq!(SECOND,9);
    assert_eq!(FIRST,b"ab");
    assert_eq!(LAST,b"cd");
    let mut data=[11u32,22,33,44];
    let start=NonNull::new(data.as_mut_ptr()).unwrap();
    let second=unsafe {start.byte_add(core::mem::size_of::<u32>())};
    assert_eq!(unsafe {second.read()},22);
    assert_eq!(unsafe {second.offset_from_unsigned(start)},1);
    assert_eq!(unsafe {second.byte_sub(core::mem::size_of::<u32>())},start);
    assert_eq!(unsafe {second.replace(25)},22);
    assert_eq!(data,[11,25,33,44]);
    unsafe {start.copy_to(start.add(1),3);}
    assert_eq!(data,[11,11,25,33]);
    let mut copied=[0u32;4];
    let destination=NonNull::new(copied.as_mut_ptr()).unwrap();
    unsafe {start.copy_to_nonoverlapping(destination,4);}
    assert_eq!(copied,data);
    unsafe {destination.write_bytes(0,4);}
    assert_eq!(copied,[0,0,0,0]);
    let raw=start.as_ptr();
    assert_eq!(unsafe {raw.byte_add(4).read()},11);
    assert_eq!(raw.wrapping_byte_add(4).wrapping_byte_sub(4),raw);
    assert_eq!(start.addr().get(),raw.addr());
    let mapped=start.map_addr(|address| core::num::NonZeroUsize::new(address.get()+4).unwrap());
    assert_eq!(unsafe {mapped.read()},11);
    assert_eq!(mapped.with_addr(start.addr()),start);
    let remapped=raw.map_addr(|address|address+8);
    assert_eq!(unsafe {*remapped},25);
    let wide:*const [u32]=&data[..2];
    assert!(core::ptr::eq(wide,&data[..2]));
    assert!(!core::ptr::eq(wide,&data[..3]));
    assert!(wide != &data[..3] as *const [u32]);
    assert!(!core::ptr::eq(wide,&data[1..3]));
    let same_object:*const dyn Value=&data[0];
    assert!(core::ptr::eq(same_object,same_object));
    let other_object:*const dyn Value=&data[1];
    assert!(!core::ptr::eq(same_object,other_object));
    // Mapping only the data address keeps the length of this two-element view.
    let wide=wide.with_addr(unsafe {raw.add(2)}.addr());
    assert_eq!(unsafe {&*wide},&[25,33]);
    let nonnull=NonNull::from(&data[..2]);
    let nonnull=nonnull.map_addr(|address|core::num::NonZeroUsize::new(address.get()+8).unwrap());
    assert_eq!(unsafe {nonnull.as_ref()},&[25,33]);
    trait Value {fn value(&self)->u32;}
    impl Value for u32 {fn value(&self)->u32 {*self}}
    let object:*const dyn Value=&data[0];
    let object=object.with_addr(unsafe {raw.add(3)}.addr());
    assert_eq!(unsafe {(&*object).value()},33);
    let slice=&mut data[..];
    assert_eq!(slice.first_chunk::<2>(),Some(&[11,11]));
    assert_eq!(slice.last_chunk::<2>(),Some(&[25,33]));
    assert!(slice.first_chunk::<5>().is_none());
    assert!(slice.last_chunk::<5>().is_none());
    assert_eq!(slice.first_chunk::<0>(),Some(&[]));
    assert_eq!(slice.last_chunk::<0>(),Some(&[]));
    slice.first_chunk_mut::<1>().unwrap()[0]=17;
    slice.last_chunk_mut::<1>().unwrap()[0]=19;
    assert_eq!(data,[17,11,25,19]);
    let mut iterator=[1,2,3,4].into_iter();
    assert_eq!(iterator.next(),Some(1));
    assert_eq!(iterator.next_back(),Some(4));
    iterator.as_mut_slice()[0]=7;
    assert_eq!(iterator.collect::<Vec<_>>(),vec![7,3]);
    let empty:&[u8]=&[];
    assert!(empty.first_chunk::<1>().is_none());
    assert!(empty.last_chunk::<1>().is_none());
    assert_eq!(empty.last_chunk::<0>(),Some(&[]));
    let zst=&[(),(),()];
    assert_eq!(zst.first_chunk::<2>(),Some(&[(),()]));
    assert_eq!(zst.last_chunk::<2>(),Some(&[(),()]));
    println!("pointer copies, address metadata, slice chunks and iterator views");
}
