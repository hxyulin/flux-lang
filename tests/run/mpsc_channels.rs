use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

struct Item {
    count: Arc<AtomicUsize>,
    value: usize,
}
impl Drop for Item {
    fn drop(&mut self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}
fn main() {
    let mut total = 0usize;
    for bound in [0usize, 1, 7] {
        let (tx, rx) = mpsc::sync_channel(bound);
        std::thread::scope(|s| {
            for producer in 0..8usize {
                let tx = tx.clone();
                s.spawn(move || {
                    for i in 0..200usize {
                        tx.send(producer * 200 + i).unwrap();
                    }
                });
            }
            drop(tx);
            let mut n = 0;
            for value in rx {
                total += value;
                n += 1;
            }
            assert_eq!(n, 1600);
        });
    }
    let (tx, rx) = mpsc::channel();
    for i in 0..500 {
        tx.send(i).unwrap();
    }
    drop(tx);
    assert_eq!(rx.iter().sum::<usize>(), (0..500).sum::<usize>());
    assert_eq!(rx.recv(), Err(mpsc::RecvError));
    assert_eq!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    let (tx, rx) = mpsc::channel::<u32>();
    assert_eq!(rx.try_recv(), Err(mpsc::TryRecvError::Empty));
    assert_eq!(rx.try_iter().next(), None);
    tx.send(19).unwrap();
    assert_eq!(rx.try_iter().next(), Some(19));
    let start = Instant::now();
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(5)),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    assert!(start.elapsed() >= Duration::from_millis(5));
    assert_eq!(
        rx.recv_timeout(Duration::ZERO),
        Err(mpsc::RecvTimeoutError::Timeout)
    );
    tx.send(23).unwrap();
    assert_eq!(rx.recv_timeout(Duration::ZERO), Ok(23));
    drop(tx);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(100)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
    let acknowledged = AtomicUsize::new(0);
    let (tx, rx) = mpsc::sync_channel(0);
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            ready_tx.send(()).unwrap();
            tx.send(31).unwrap();
            acknowledged.store(1, Ordering::SeqCst);
        });
        ready_rx.recv().unwrap();
        assert_eq!(acknowledged.load(Ordering::SeqCst), 0);
        assert_eq!(rx.recv(), Ok(31));
    });
    assert_eq!(acknowledged.load(Ordering::SeqCst), 1);
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(7).unwrap();
    assert_eq!(tx.try_send(9), Err(mpsc::TrySendError::Full(9)));
    assert_eq!(rx.recv(), Ok(7));
    tx.try_send(11).unwrap();
    assert_eq!(rx.try_recv(), Ok(11));
    drop(rx);
    assert_eq!(tx.try_send(13), Err(mpsc::TrySendError::Disconnected(13)));
    assert_eq!(tx.send(17), Err(mpsc::SendError(17)));
    let (tx, rx) = mpsc::sync_channel::<u32>(0);
    assert_eq!(tx.try_send(3), Err(mpsc::TrySendError::Full(3)));
    drop(rx);
    assert_eq!(tx.send(4), Err(mpsc::SendError(4)));
    let drops = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel();
    for value in 0..21 {
        tx.send(Item {
            count: drops.clone(),
            value,
        })
        .unwrap();
    }
    assert_eq!(rx.recv().unwrap().value, 0);
    drop(rx);
    assert_eq!(drops.load(Ordering::SeqCst), 21);
    let item = tx
        .send(Item {
            count: drops.clone(),
            value: 99,
        })
        .unwrap_err()
        .0;
    assert_eq!(item.value, 99);
    drop(item);
    for bound in [0usize, 1] {
        let (tx, rx) = mpsc::sync_channel(bound);
        let (ready_tx, ready_rx) = mpsc::channel();
        if bound == 1 {
            tx.send(Item {
                count: drops.clone(),
                value: 1,
            })
            .unwrap();
        }
        std::thread::scope(|s| {
            let count = drops.clone();
            let worker = s.spawn(move || {
                ready_tx.send(()).unwrap();
                tx.send(Item { count, value: 55 }).unwrap_err().0
            });
            ready_rx.recv().unwrap();
            drop(rx);
            let item = worker.join().unwrap();
            assert_eq!(item.value, 55);
            drop(item);
        });
    }
    assert_eq!(drops.load(Ordering::SeqCst), 25);
    println!(
        "channels {} {} {:?} {:?} {}",
        total,
        drops.load(Ordering::SeqCst),
        mpsc::TrySendError::Full(3),
        mpsc::TrySendError::Disconnected(3),
        mpsc::RecvTimeoutError::Timeout
    );
}
