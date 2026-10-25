use std::sync::atomic::{AtomicUsize, Ordering};

struct Count<'a>(&'a AtomicUsize);
impl Drop for Count<'_> {
    fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); }
}

fn main() {
    let ready = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    let drops = AtomicUsize::new(0);
    let mut value = 12;
    let text = String::from("borrowed");
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..4 {
            let guard = Count(&drops);
            let ready = &ready;
            let finished = &finished;
            handles.push(scope.spawn(move || {
                ready.fetch_add(1, Ordering::SeqCst);
                // This cannot finish if spawn executes closures sequentially.
                while ready.load(Ordering::SeqCst) != 4 {}
                finished.fetch_add(1, Ordering::SeqCst);
                drop(guard);
                1
            }));
        }
        for handle in handles { assert_eq!(handle.join().unwrap(), 1); }
        scope.spawn(|| { value += 5; });
        assert_eq!(scope.spawn(|| text.as_str()).join().unwrap(), "borrowed");
        let done = &finished;
        // Nested workers are registered by their parent while scope cleanup
        // may already be joining it.
        scope.spawn(move || { scope.spawn(move || { done.fetch_add(1, Ordering::SeqCst); }); });
        let guard = Count(&drops);
        drop(scope.spawn(move || guard));
        let guard = Count(&drops);
        drop(scope.spawn(move || { drop(guard); }));
    });
    assert_eq!(ready.load(Ordering::SeqCst), 4);
    assert_eq!(finished.load(Ordering::SeqCst), 5);
    assert_eq!(drops.load(Ordering::SeqCst), 6);
    assert_eq!(value, 17);
    println!("scoped 4 5 6 17");
}
