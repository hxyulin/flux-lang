use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr,Ordering};
const NULL:*mut c_void=std::ptr::null_mut();
const INVALID:*mut c_void=1usize as *mut c_void;
const SAME:*mut c_void=1usize as *mut c_void;
fn classify(pointer:*mut c_void)->u8{match pointer{NULL=>0,INVALID=>1,_=>2}}
fn duplicate(pointer:*mut c_void)->u8{match pointer{INVALID=>7,SAME=>8,_=>9}}
const PAIR:(*const u8,u8)=(1usize as *const u8,7);
fn aggregate(value:(*const u8,u8))->bool{matches!(value,PAIR)}
#[derive(PartialEq,Eq)]struct Record{pointer:*const u8,tag:u8}
const RECORD:Record=Record{pointer:1usize as *const u8,tag:9};
fn record(value:Record)->bool{matches!(value,RECORD)}
fn main(){
 let cell=AtomicPtr::<c_void>::new(INVALID);assert_eq!(classify(cell.load(Ordering::Relaxed)),1);
 cell.store(NULL,Ordering::Relaxed);assert_eq!(classify(cell.load(Ordering::Relaxed)),0);
 assert_eq!(classify(42usize as *mut c_void),2);assert_eq!(classify(usize::MAX as *mut c_void),2);
 assert_eq!(duplicate(INVALID),7);assert_eq!(duplicate(NULL),9);
 assert!(aggregate((1usize as *const u8,7)));assert!(!aggregate((2usize as *const u8,7)));
 assert!(record(Record{pointer:1usize as *const u8,tag:9}));assert!(!record(Record{pointer:1usize as *const u8,tag:8}));
 println!("{} {} {}",classify(INVALID),classify(NULL),duplicate(SAME));
}
