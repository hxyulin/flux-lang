use std::{cell::Cell, mem, ptr, rc::Rc};
fn through_bytes<T>(value:T)->T{
    let size=mem::size_of::<T>();let mut bytes=vec![0u8;size+1];
    // SAFETY: the byte ranges are valid and disjoint. Raw copying may carry
    // padding; it is never read as u8. Forget the old owner after transferring it.
    unsafe{ptr::copy_nonoverlapping((&raw const value).cast::<u8>(),bytes.as_mut_ptr().add(1),size);}
    // SAFETY: the copied storage represents the initialized T and needs no alignment.
    let result=unsafe{ptr::read_unaligned(bytes.as_ptr().add(1).cast::<T>())};
    mem::forget(value);result
}
#[repr(C)]#[derive(Debug,PartialEq)]struct Padded{byte:u8,word:u64}
#[repr(align(64))]struct Aligned(u64);
struct Owner(Rc<Cell<u32>>);
impl Drop for Owner{fn drop(&mut self){self.0.set(self.0.get()+1);}}
fn main(){
    assert_eq!(through_bytes(-1234567i32),-1234567);assert!(through_bytes(true));
    assert_eq!(through_bytes('🦀'),'🦀');assert_eq!(through_bytes(-1.25f64),-1.25);
    assert_eq!(through_bytes(u128::MAX),u128::MAX);
    assert_eq!(through_bytes(Padded{byte:7,word:123456789}),Padded{byte:7,word:123456789});
    assert_eq!(through_bytes(Aligned(91)).0,91);
    assert_eq!(through_bytes([1u16,2,3]),[1,2,3]);
    assert_eq!(through_bytes((7u8,11u64)),(7,11));
    let bytes:&[u8]=b"unaligned";assert_eq!(through_bytes(bytes),bytes);
    let value=17u32;
    // SAFETY: copying the pointer preserves its address; value remains live and initialized.
    assert_eq!(unsafe{*through_bytes(&raw const value)},17);
    assert_eq!(through_bytes(String::from("owner")),"owner");
    assert_eq!(through_bytes(Some(std::num::NonZeroU32::new(7).unwrap())).unwrap().get(),7);
    assert_eq!(through_bytes(None::<std::num::NonZeroU32>),None);
    through_bytes(());let drops=Rc::new(Cell::new(0));drop(through_bytes(Owner(drops.clone())));assert_eq!(drops.get(),1);
    println!("unaligned represented values and ownership");
}
