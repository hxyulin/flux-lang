use core::sync::atomic::*;
use core::sync::atomic::Ordering::*;
macro_rules! integer {
    ($atomic:ident,$scalar:ty)=>{{
        for success in [Relaxed,Acquire,Release,AcqRel,SeqCst] {
            for failure in [Relaxed,Acquire,SeqCst] {
                let value=$atomic::new(7);
                assert_eq!(value.compare_exchange(7,9,success,failure),Ok(7));
                assert_eq!(value.compare_exchange(7,11,success,failure),Err(9));
                loop {
                    match value.compare_exchange_weak(9,12,success,failure) {
                        Ok(old)=>{assert_eq!(old,9);break;},Err(old)=>assert_eq!(old,9),
                    }
                }
                assert_eq!(value.load(Relaxed),12);
            }
        }
        let value=$atomic::new(0);
        assert_eq!(value.fetch_sub(1,AcqRel),0);
        assert_eq!(value.load(Relaxed),<$scalar>::MAX.wrapping_sub(<$scalar>::MIN));
        assert_eq!(value.fetch_add(2,Release),(-1i64) as $scalar);
        assert_eq!(value.swap(0x55,Acquire),1);
        assert_eq!(value.fetch_and(0x0f,SeqCst),0x55);
        assert_eq!(value.fetch_or(0x20,SeqCst),5);
        assert_eq!(value.fetch_xor(0x03,SeqCst),0x25);
        assert_eq!(value.fetch_nand(0x0f,SeqCst),0x26);
        let before=value.load(Relaxed);
        let mut calls=0;
        assert_eq!(value.fetch_update(AcqRel,Acquire,|old|{calls+=1;Some(old.wrapping_add(3))}),Ok(before));
        assert_eq!(calls,1);
        let before=value.load(Relaxed);
        assert_eq!(value.fetch_update(Release,Relaxed,|_|None),Err(before));
        value.store(3,Relaxed);
        assert_eq!(value.fetch_max(7,Relaxed),3);
        assert_eq!(value.fetch_min(5,Relaxed),7);
        assert_eq!(value.load(Relaxed),5);
        let mut raw: $scalar=19;
        // SAFETY: raw remains live with the atomic's natural alignment and is
        // accessed exclusively through this atomic view until the borrow ends.
        let view=unsafe {$atomic::from_ptr(&raw mut raw)};
        assert_eq!(view.swap(23,SeqCst),19);
        assert_eq!(raw,23);
    }};
}
fn contention() {
    let value=std::sync::Arc::new(AtomicU16::new(0));
    let calls=std::sync::Arc::new(AtomicUsize::new(0));
    let mut workers=Vec::new();
    for _ in 0..4 {
        let value=value.clone();let calls=calls.clone();
        workers.push(std::thread::spawn(move || {
            for _ in 0..1000 {
                let _=value.fetch_update(AcqRel,Acquire,|old| {
                    calls.fetch_add(1,Relaxed);
                    Some(old+1)
                }).unwrap();
            }
        }));
    }
    for worker in workers {worker.join().unwrap();}
    assert_eq!(value.load(SeqCst),4000);
    assert!(calls.load(Relaxed)>=4000);
}
fn main() {
    contention();
    integer!(AtomicU8,u8);integer!(AtomicU16,u16);integer!(AtomicU32,u32);integer!(AtomicU64,u64);integer!(AtomicUsize,usize);
    integer!(AtomicI8,i8);integer!(AtomicI16,i16);integer!(AtomicI32,i32);integer!(AtomicI64,i64);integer!(AtomicIsize,isize);
    let signed=AtomicI16::new(-9);
    assert_eq!(signed.fetch_max(-3,AcqRel),-9);
    assert_eq!(signed.fetch_min(-7,AcqRel),-3);
    assert_eq!(signed.load(Relaxed),-7);
    let flag=AtomicBool::new(false);
    assert_eq!(flag.compare_exchange(false,true,AcqRel,Acquire),Ok(false));
    assert!(flag.fetch_nand(true,Relaxed));
    assert!(!flag.load(Relaxed));
    assert!(!flag.fetch_or(true,Relaxed));
    assert!(flag.fetch_xor(true,Relaxed));
    assert!(!flag.swap(true,Relaxed));
    assert_eq!(flag.fetch_update(SeqCst,Acquire,|_|None),Err(true));
    let mut values=[1u32,2];
    let first=values.as_mut_ptr();
    // SAFETY: both pointers stay within this live array.
    let second=unsafe {first.add(1)};
    let pointer=AtomicPtr::new(first);
    assert_eq!(pointer.compare_exchange(first,second,SeqCst,Acquire),Ok(first));
    assert_eq!(pointer.compare_exchange(first,core::ptr::null_mut(),Release,Relaxed),Err(second));
    assert_eq!(pointer.fetch_update(AcqRel,Acquire,|_|Some(first)),Ok(second));
    assert_eq!(pointer.swap(second,Acquire),first);
    assert_eq!(unsafe {*pointer.load(Acquire)},2);
    #[repr(C)]struct Neighbors {left:u16,value:AtomicU16,right:u16}
    let neighbors=Neighbors{left:0x1234,value:AtomicU16::new(0xabcd),right:0x5678};
    assert_eq!(neighbors.value.compare_exchange(0xabcd,0xffff,SeqCst,SeqCst),Ok(0xabcd));
    assert_eq!((neighbors.left,neighbors.right),(0x1234,0x5678));
    println!("atomic update widths, signed comparisons, pointers and order pairs");
}
