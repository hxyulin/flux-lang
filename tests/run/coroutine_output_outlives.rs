use std::{future::Future,pin::Pin,task::{Context,Poll,Wake,Waker},sync::Arc};
struct Holder<T>(Pin<Box<dyn Future<Output=T>+Send+'static>>);
fn empty<T>()->Holder<T>{Holder(Box::pin(async{unreachable!()}))}
fn ready<T>()->Holder<T> where T:Default+'static+Send{Holder(Box::pin(async{T::default()}))}
struct Noop;
impl Wake for Noop{fn wake(self:Arc<Self>) {}}
fn main(){let unused=empty::<&u8>();drop(unused);let waker=Waker::from(Arc::new(Noop));let mut cx=Context::from_waker(&waker);let mut future=ready::<u8>();assert!(matches!(future.0.as_mut().poll(&mut cx),Poll::Ready(0)));println!("future output lifetime");}
