use std::sync::atomic::{AtomicUsize, Ordering};

static DROPPED: AtomicUsize = AtomicUsize::new(0);

struct Payload([usize; 30], usize);

impl Drop for Payload {
    fn drop(&mut self) {
        DROPPED.fetch_add(self.1, Ordering::Relaxed);
    }
}

enum Output {
    Empty(usize),
    Full(Payload, usize),
}

#[inline(never)]
fn make_payload(seed: usize) -> Payload {
    let mut words = [seed; 30];
    words[29] += 1;
    Payload(words, seed)
}

#[inline(never)]
fn build_output(seed: usize) -> Output {
    if seed == 0 {
        Output::Empty(17)
    } else {
        Output::Full(make_payload(seed), 11)
    }
}

fn main() {
    match build_output(0) {
        Output::Empty(value) => assert_eq!(value, 17),
        Output::Full(_, _) => panic!("expected empty"),
    }
    match build_output(9) {
        Output::Empty(_) => panic!("expected payload"),
        Output::Full(payload, label) => {
            assert_eq!(payload.0[0], 9);
            assert_eq!(payload.0[29], 10);
            assert_eq!(label, 11);
            assert_eq!(DROPPED.load(Ordering::Relaxed), 0);
            drop(payload);
        }
    }
    println!("{}", DROPPED.load(Ordering::Relaxed));
}
