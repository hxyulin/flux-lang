fn main(){
    let main=std::thread::current().id();
    assert_eq!(main,std::thread::current().clone().id());
    let mut ids=std::collections::HashSet::new();
    for _ in 0..8 {
        let id=std::thread::spawn(|| {let id=std::thread::current().id();assert_eq!(id,std::thread::current().id());id}).join().unwrap();
        assert_ne!(id,main);assert!(ids.insert(id));
    }
    println!("{}",ids.len());
}
