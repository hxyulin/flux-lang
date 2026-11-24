use std::sync::atomic::{AtomicUsize, Ordering};

static DROPPED: AtomicUsize = AtomicUsize::new(0);

struct Count(usize);

impl Drop for Count {
    fn drop(&mut self) {
        DROPPED.fetch_add(self.0, Ordering::Relaxed);
    }
}

enum Action {
    Idle,
    Own(Count),
}

#[inline(never)]
fn make(kind: usize) -> Action {
    if kind == 0 {
        Action::Idle
    } else {
        Action::Own(Count(kind))
    }
}

#[inline(never)]
fn replace(value: &mut Action) {
    *value = make(2);
}

#[inline(never)]
fn observe(value: usize) -> usize {
    std::hint::black_box(value)
}

#[inline(never)]
fn direct(kind: usize) -> usize {
    let mut value = make(kind);
    match value {
        Action::Idle => {
            value = make(3);
            return observe(kind);
        }
        Action::Own(_) => return 99,
    }
}

#[inline(never)]
fn through_alias(kind: usize) -> usize {
    let mut value = make(kind);
    match value {
        Action::Idle => {
            replace(&mut value);
            return observe(kind);
        }
        Action::Own(_) => return 99,
    }
}

#[inline(never)]
fn preserve(kind: usize) -> usize {
    let value = make(kind);
    match value {
        Action::Idle => observe(kind),
        Action::Own(owned) => observe(owned.0),
    }
}

fn main() {
    assert_eq!(preserve(0), 0);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
    assert_eq!(direct(0), 0);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 3);
    assert_eq!(through_alias(0), 0);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 5);
    assert_eq!(direct(4), 99);
    assert_eq!(DROPPED.load(Ordering::Relaxed), 9);
    println!("{}", DROPPED.load(Ordering::Relaxed));
}
