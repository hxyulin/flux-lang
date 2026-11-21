use std::sync::{Arc, RwLock, RwLockWriteGuard, TryLockError};

fn main() {
    let shared = Arc::new(RwLock::new(Vec::<u32>::new()));
    let mut writer = shared.write().unwrap();
    writer.push(23);
    let reader = RwLockWriteGuard::downgrade(writer);
    assert_eq!(reader[0], 23);

    let peer = Arc::clone(&shared);
    let check = std::thread::spawn(move || {
        assert!(matches!(peer.try_write(), Err(TryLockError::WouldBlock)));
        let peer_reader = peer.try_read().unwrap();
        assert_eq!(peer_reader[0], 23);
    });
    check.join().unwrap();
    drop(reader);

    let mut writer = shared.try_write().unwrap();
    writer.push(42);
    assert_eq!(writer.len(), 2);
    println!("{} {}", writer[0], writer[1]);
}
