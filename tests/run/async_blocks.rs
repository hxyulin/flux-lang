use core::future::Future;
use core::task::{Context,Poll,Waker};
use core::pin::{Pin,pin};
use core::cell::Cell;
struct Yield(bool);
impl Future for Yield {
    type Output=u32;
    fn poll(self:Pin<&mut Self>,_:&mut Context<'_>)->Poll<u32> {
        let this=self.get_mut();
        if this.0 { this.0=false; Poll::Pending } else { Poll::Ready(7) }
    }
}
struct Guard<'a>(u32,&'a core::cell::RefCell<Vec<u32>>);
impl Drop for Guard<'_> {fn drop(&mut self) {self.1.borrow_mut().push(self.0);}}
struct YieldGuard<'a> {pending:bool,guard:Guard<'a>}
impl Future for YieldGuard<'_> {
    type Output=u32;
    fn poll(self:Pin<&mut Self>,_:&mut Context<'_>)->Poll<u32> {
        let this=self.get_mut();
        if this.pending {this.pending=false;Poll::Pending} else {Poll::Ready(7)}
    }
}
fn cancellation(polls:u32) {
    let log=core::cell::RefCell::new(Vec::new());
    {
        let capture=Guard(0,&log);
        let events=&log;
        let future=async move {
            let _capture=capture;
            let _one=Guard(1,events);
            YieldGuard{pending:true,guard:Guard(2,events)}.await;
            let _three=Guard(3,events);
            YieldGuard{pending:true,guard:Guard(4,events)}.await;
            12u32
        };
        let mut future=pin!(future);
        let mut context=Context::from_waker(Waker::noop());
        for i in 0..polls {
            let result=future.as_mut().poll(&mut context);
            if i==2 {assert_eq!(result,Poll::Ready(12));} else {assert!(result.is_pending());}
        }
    }
    let expected=match polls {0=>vec![0],1=>vec![2,1,0],_=>vec![2,4,3,1,0]};
    assert_eq!(*log.borrow(),expected);
}
fn frame_locals() {
    let mut context=Context::from_waker(Waker::noop());
    let mut future=pin!(async {
        let mut values=[1u32,2u32];
        let source=[3u32,11u32];
        let borrowed=&source[1];
        for i in 0..2 {values[i]+=Yield(true).await;}
        values[0]+values[1]+*borrowed
    });
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(future.as_mut().poll(&mut context),Poll::Ready(28));
}
fn partial_moves() {
    let log=core::cell::RefCell::new(Vec::new());
    {
        let pair=(Guard(1,&log),Guard(2,&log));
        let mut future=pin!(async move {
            let first=pair.0;
            let second=pair.1;
            drop(first);
            Yield(true).await;
            drop(second);
        });
        let mut context=Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(*log.borrow(),vec![1]);
    }
    assert_eq!(*log.borrow(),vec![1,2]);
}
fn nested_and_outputs() {
    let log=core::cell::RefCell::new(Vec::new());
    let mut context=Context::from_waker(Waker::noop());
    {
        let mut future=pin!(async {
            let value=(async {Yield(true).await+1}).await;
            let output=(async {Guard(value,&log)}).await;
            output
        });
        assert!(future.as_mut().poll(&mut context).is_pending());
        let output=match future.as_mut().poll(&mut context) {Poll::Ready(value)=>value,Poll::Pending=>panic!("not ready")};
        assert!(log.borrow().is_empty());
        drop(output);
        assert_eq!(*log.borrow(),vec![8]);
    }
    assert_eq!(*log.borrow(),vec![8]);
    let mut ready=pin!(async {13u32});
    assert_eq!(ready.as_mut().poll(&mut context),Poll::Ready(13));
    {
        let mut future=pin!(async {
            let _guard=Guard(17,&log);
            (async {Yield(true).await}).await
        });
        assert!(future.as_mut().poll(&mut context).is_pending());
    }
    assert_eq!(*log.borrow(),vec![8,17]);
}
fn conditional_drops() {
    for drop_first in [false,true] {
        let log=core::cell::RefCell::new(Vec::new());
        {
            let owned=Guard(3,&log);
            let mut future=pin!(async move {
                let value=owned;
                if drop_first {drop(value);}
                Yield(true).await;
            });
            let mut context=Context::from_waker(Waker::noop());
            assert!(future.as_mut().poll(&mut context).is_pending());
        }
        assert_eq!(*log.borrow(),vec![3]);
    }
}
struct AddressGuard<'a> {address:Cell<usize>,drops:&'a Cell<u32>,_pin:core::marker::PhantomPinned}
impl Drop for AddressGuard<'_> {
    fn drop(&mut self) {
        assert_eq!(self as *const Self as usize,self.address.get());
        self.drops.set(self.drops.get()+1);
    }
}
fn pinned_local_drop() {
    let drops=Cell::new(0u32);
    {
        let mut future=pin!(async {
            let guard=AddressGuard{address:Cell::new(0usize),drops:&drops,_pin:core::marker::PhantomPinned};
            guard.address.set(&guard as *const AddressGuard<'_> as usize);
            Yield(true).await;
            let _=guard.address.get();
        });
        let mut context=Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
    }
    assert_eq!(drops.get(),1);
}
fn round_robin() {
    let lanes=[0u32,1,2].map(|index| Some(async move {Yield(true).await+index}));
    let mut lanes=pin!(lanes);
    let mut context=Context::from_waker(Waker::noop());
    let mut outputs=[0u32;3];
    loop {
        let mut complete=0;
        for i in 0..3 {
            // SAFETY: every lane remains in this pinned array until it is
            // dropped in place by setting its optional slot to None.
            let mut slot=unsafe {lanes.as_mut().map_unchecked_mut(|lanes|&mut lanes[i])};
            if let Some(mut lane)=slot.as_mut().as_pin_mut() {
                if let Poll::Ready(value)=lane.as_mut().poll(&mut context) {
                    outputs[i]=value;
                    slot.set(None);
                    complete+=1;
                }
            } else {complete+=1;}
        }
        if complete==3 {break;}
    }
    assert_eq!(outputs,[7,8,9]);
}
fn main() {
    round_robin();
    for polls in 0..=3 {cancellation(polls);}
    nested_and_outputs();
    conditional_drops();
    pinned_local_drop();
    frame_locals();
    partial_moves();
    let starts=Cell::new(0u32);
    let mut context=Context::from_waker(Waker::noop());
    let mut future=pin!(async {
        starts.set(starts.get()+1);
        let value=Yield(true).await;
        value+5
    });
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(starts.get(),1);
    assert_eq!(future.as_mut().poll(&mut context),Poll::Ready(12));
    assert_eq!(starts.get(),1);
    println!("async resumed 12 once");
}
