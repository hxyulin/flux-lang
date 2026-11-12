use std::sync::{RwLock,RwLockWriteGuard};
struct Resource(RwLock<u8>);
struct Guard<'a>(RwLockWriteGuard<'a,u8>);
fn check(flag:bool){
    let value=Resource(RwLock::new(7));
    let guard=Guard(value.0.write().unwrap());
    if flag{println!("{}",*guard.0);drop(guard);drop(value);}
}
fn main(){check(true);check(false);println!("conditional drops passed");}
