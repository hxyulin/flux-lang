// Buffered writers, vectored reads, io helpers, file copies and links,
// condition variables, thread parking and names, and legacy error APIs.
#![allow(deprecated)]
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::prelude::*;
use std::io::{self, BufWriter, Cursor, IoSliceMut, LineWriter};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::sync::{Arc, Condvar, Mutex, RwLock, TryLockError};
use std::thread;
use std::time::Duration;

#[derive(Debug)]
struct Outer(Inner);
#[derive(Debug)]
struct Inner;
impl fmt::Display for Outer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("outer") }
}
impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("inner") }
}
impl Error for Inner {}
impl Error for Outer {
    fn description(&self) -> &str { "outer description" }
    fn cause(&self) -> Option<&dyn Error> { Some(&self.0) }
}

fn parse_twice(text: &str) -> Result<i32, std::num::ParseIntError> {
    let value: i32 = r#try!(text.parse());
    Ok(r#try!(text.parse::<i32>(),) + value)
}

fn main() {
    let outer = Outer(Inner);
    println!("{} {:?} {}", outer.description(), outer.cause().map(|e| e.to_string()), outer.source().is_none());
    println!("{:?} {}", Inner.cause().is_none(), Inner.description());
    println!("{:?} {:?}", parse_twice("21"), parse_twice("x").is_err());

    let mut writer = BufWriter::with_capacity(4, Vec::new());
    writer.write_all(b"ab").unwrap();
    println!("{:?} {}", writer.buffer(), writer.get_ref().len());
    writer.write_all(b"cdef").unwrap();
    println!("{:?} {:?} {}", writer.buffer(), writer.get_ref(), writer.capacity());
    write!(writer, "{}", 7).unwrap();
    println!("{:?}", writer);
    let inner = writer.into_inner().unwrap();
    println!("{:?}", inner);
    let mut lines = LineWriter::new(Vec::new());
    lines.write_all(b"one\ntw").unwrap();
    println!("{:?}", lines.get_ref());
    lines.write_all(b"o\n").unwrap();
    println!("{:?}", lines.get_ref());

    let mut first = [0u8; 2];
    let mut second = [0u8; 3];
    let mut source: &[u8] = b"vectored";
    let count = source.read_vectored(&mut [IoSliceMut::new(&mut first), IoSliceMut::new(&mut second)]).unwrap();
    println!("{} {:?} {:?}", count, first, second);
    let mut chained = String::new();
    (&b"left "[..]).chain(Cursor::new(b"right")).read_to_string(&mut chained).unwrap();
    println!("{}", chained);
    let mut sink = Vec::new();
    println!("{} {:?}", io::copy(&mut io::repeat(b'z').take(3), &mut sink).unwrap(), sink);
    println!("{} {:?}", io::copy(&mut io::empty(), &mut sink).unwrap(), io::read_to_string(&b"text"[..]).unwrap());
    let lines: Vec<String> = Cursor::new("a\r\nb\n\nc").lines().map(|line| line.unwrap()).collect();
    let parts: Vec<Vec<u8>> = Cursor::new("x,y,").split(b',').map(|part| part.unwrap()).collect();
    println!("{:?} {:?}", lines, parts);
    let error = io::Error::other("custom");
    println!("{:?} {}", error.kind(), error);

    let root = std::env::temp_dir().join(format!("flux-io-fs-sync-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let original = root.join("original.txt");
    let mut file = fs::OpenOptions::new().write(true).create(true).mode(0o640).open(&original).unwrap();
    file.write_all(b"contents").unwrap();
    file.sync_data().unwrap();
    drop(file);
    let copy = root.join("copy.txt");
    println!("{} {:?}", fs::copy(&original, &copy).unwrap(), fs::read_to_string(&copy).unwrap());
    let metadata = fs::metadata(&copy).unwrap();
    println!("{:o} {} {} {}", metadata.permissions().mode() & 0o777, metadata.size(), metadata.nlink(), metadata.modified().is_ok());
    let hard = root.join("hard.txt");
    fs::hard_link(&original, &hard).unwrap();
    println!("{} {}", fs::metadata(&original).unwrap().nlink(), fs::read_to_string(&hard).unwrap());
    let soft = root.join("soft.txt");
    std::os::unix::fs::symlink("original.txt", &soft).unwrap();
    println!("{:?} {} {}", fs::read_link(&soft).unwrap(), soft.is_symlink(), fs::exists(&soft).unwrap());
    println!("{:?}", fs::copy(&root, root.join("dir-copy")).map_err(|error| error.kind()));
    fs::remove_dir_all(&root).unwrap();

    let pair = Arc::new((Mutex::new(0), Condvar::new()));
    let workers: Vec<_> = (0..3).map(|index| {
        let pair = pair.clone();
        thread::Builder::new().name(format!("worker-{index}")).spawn(move || {
            let (lock, ready) = &*pair;
            *lock.lock().unwrap() += 1;
            ready.notify_all();
            thread::current().name().unwrap().to_string()
        }).unwrap()
    }).collect();
    let (lock, ready) = &*pair;
    let guard = ready.wait_while(lock.lock().unwrap(), |count| *count < 3).unwrap();
    println!("{}", *guard);
    drop(guard);
    for worker in workers { println!("{}", worker.join().unwrap()); }
    let (guard, timeout) = ready.wait_timeout(lock.lock().unwrap(), Duration::from_millis(5)).unwrap();
    println!("{} {}", *guard, timeout.timed_out());
    match lock.try_lock() {
        Err(TryLockError::WouldBlock) => println!("would block"),
        _ => println!("locked"),
    }
    drop(guard);
    println!("{}", lock.try_lock().is_ok());
    let shared = RwLock::new(vec![1]);
    let reader = shared.read().unwrap();
    println!("{} {}", shared.try_read().is_ok(), shared.try_write().is_err());
    println!("{:?}", shared);
    drop(reader);

    let parked = thread::spawn(|| {
        thread::park();
        "unparked"
    });
    thread::sleep(Duration::from_millis(10));
    parked.thread().unpark();
    println!("{}", parked.join().unwrap());
    thread::current().unpark();
    thread::park();
    thread::park_timeout(Duration::from_millis(1));
    thread::yield_now();
    println!("{:?} {}", thread::current().name(), thread::current().id() == thread::current().id());
    println!("{:?}", std::panic::catch_unwind(|| 3));
}
