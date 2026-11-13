use std::os::unix::prelude::*;
fn main() {
    let thread = std::thread::spawn(|| 7);
    assert_ne!(thread.as_pthread_t() as usize,0);
    assert_eq!(thread.join().unwrap(),7);
    println!("thread handle");
}
