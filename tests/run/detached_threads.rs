use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

struct Signal(mpsc::Sender<()>);
impl Drop for Signal {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}
thread_local! { static TLS: RefCell<Option<Signal>> = RefCell::new(None); }
struct Value {
    value: usize,
    drops: Arc<AtomicUsize>,
    done: mpsc::Sender<()>,
}
impl Drop for Value {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        self.done.send(()).unwrap();
    }
}
struct Environment(Arc<AtomicUsize>);
impl Drop for Environment {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
fn main() {
    let values = Arc::new(AtomicUsize::new(0));
    let environments = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (tls_tx, tls_rx) = mpsc::channel();
    let count = values.clone();
    let env = Environment(environments.clone());
    let handle = std::thread::spawn(move || {
        TLS.with(|slot| *slot.borrow_mut() = Some(Signal(tls_tx)));
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        drop(env);
        Value {
            value: 17,
            drops: count,
            done: done_tx,
        }
    });
    started_rx.recv().unwrap();
    assert!(!handle.is_finished());
    drop(handle);
    release_tx.send(()).unwrap();
    done_rx.recv().unwrap();
    tls_rx.recv().unwrap();
    assert_eq!(values.load(Ordering::SeqCst), 1);
    assert_eq!(environments.load(Ordering::SeqCst), 1);
    let (done_tx, done_rx) = mpsc::channel();
    let (tls_tx, tls_rx) = mpsc::channel();
    let count = values.clone();
    let env = Environment(environments.clone());
    let handle = std::thread::spawn(move || {
        TLS.with(|slot| *slot.borrow_mut() = Some(Signal(tls_tx)));
        drop(env);
        Value {
            value: 23,
            drops: count,
            done: done_tx,
        }
    });
    tls_rx.recv().unwrap();
    assert!(handle.is_finished());
    assert_eq!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(values.load(Ordering::SeqCst), 1);
    drop(handle);
    done_rx.recv().unwrap();
    assert_eq!(values.load(Ordering::SeqCst), 2);
    let (done_tx, done_rx) = mpsc::channel();
    let (tls_tx, tls_rx) = mpsc::channel();
    let count = values.clone();
    let handle = std::thread::spawn(move || {
        TLS.with(|slot| *slot.borrow_mut() = Some(Signal(tls_tx)));
        Value {
            value: 31,
            drops: count,
            done: done_tx,
        }
    });
    let result: std::thread::Result<Value> = handle.join();
    let value = result.unwrap();
    assert_eq!(value.value, 31);
    tls_rx.try_recv().unwrap();
    assert_eq!(values.load(Ordering::SeqCst), 2);
    drop(value);
    done_rx.recv().unwrap();
    assert_eq!(values.load(Ordering::SeqCst), 3);
    let handles = (0..8usize)
        .map(|n| std::thread::spawn(move || std::thread::spawn(move || n * 2).join().unwrap() + 1))
        .collect::<Vec<_>>();
    let mut total = 0;
    for handle in handles {
        total += handle.join().unwrap();
    }
    assert_eq!(total, 64);
    let scoped = std::thread::scope(|scope| {
        let handle = scope.spawn(|| 19);
        handle.join()
    });
    let _: std::thread::Result<u32> = scoped;
    println!(
        "detached threads {} {} {}",
        values.load(Ordering::SeqCst),
        environments.load(Ordering::SeqCst),
        total
    );
}
