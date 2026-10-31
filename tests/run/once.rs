use std::sync::Once;
use std::sync::atomic::{AtomicUsize,Ordering};
static INIT: Once = Once::new();
static COUNT: AtomicUsize = AtomicUsize::new(0);
fn main(){
    assert!(!INIT.is_completed());
    let mut threads=Vec::new();
    for _ in 0..8 {threads.push(std::thread::spawn(||INIT.call_once(||{COUNT.fetch_add(1,Ordering::SeqCst);})));}
    for thread in threads {thread.join().unwrap();}
    assert!(INIT.is_completed());
    INIT.call_once(||panic!("already initialized"));
    let other=Once::new();
    other.call_once_force(|state|assert!(!state.is_poisoned()));
    println!("{} {}",COUNT.load(Ordering::SeqCst),other.is_completed());
}
