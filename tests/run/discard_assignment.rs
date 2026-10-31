use std::sync::atomic::{AtomicUsize,Ordering};
static DROPS:AtomicUsize=AtomicUsize::new(0);
struct Value;
impl Drop for Value {fn drop(&mut self){DROPS.fetch_add(1,Ordering::SeqCst);}}
fn main(){
    let uninitialized: String;
    _ = uninitialized;
    let value=Value;
    _ = value;
    assert_eq!(DROPS.load(Ordering::SeqCst),0);
    drop(value);
    _ = &Value;
    assert_eq!(DROPS.load(Ordering::SeqCst),2);
    _ = Value;
    let x = (_ = String::from("discarded"));
    assert_eq!(x,());
    println!("{}",DROPS.load(Ordering::SeqCst));
}
