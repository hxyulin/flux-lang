//! `Vec`: growth and `capacity()` as rustc's `RawVec` has them (for
//! elements of 1, 4 and 2048 bytes and of no size), `push`, `pop`,
//! `insert`, `remove`, `swap_remove`, `truncate`, `clear`, `retain`,
//! `dedup`, `resize`, `extend_from_slice`, `append`, `split_off`,
//! `reserve`, `reserve_exact`, `shrink_to_fit`, slice methods through
//! `Deref`, indexing and ranges, iteration by value, by reference and
//! mutably, `Clone`, comparisons, `Debug`, `Default`, `vec!` in both
//! forms, and drops in rustc's order.

struct Noisy(i32);

impl Drop for Noisy {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Big([u8; 2048]);

fn caps<T>(make: fn(usize) -> T, n: usize) {
    let mut v = Vec::new();
    let mut last = v.capacity();
    print!("{last}");
    for i in 0..n {
        v.push(make(i));
        if v.capacity() != last {
            last = v.capacity();
            print!(" {last}");
        }
    }
    println!();
}

fn main() {
    caps(|i| i as u8, 40);
    caps(|i| i as u32, 40);
    caps(|_| Big([0; 2048]), 6);
    caps(|_| (), 3);

    let mut v: Vec<i32> = vec![0, 10, 20, 30, 40];
    v.push(50);
    v.insert(0, -10);
    println!("{v:?} len {} cap {}", v.len(), v.capacity());
    println!("{} {:?} {:?}", v.remove(1), v.pop(), v.first());
    println!("{} {v:?}", v.swap_remove(0));
    v.retain(|&x| x != 20);
    v.extend_from_slice(&[7, 7, 8]);
    v.dedup();
    println!("{v:?} {}", v.capacity());
    v.resize(8, 1);
    v.truncate(7);
    let tail = v.split_off(5);
    println!("{v:?} {tail:?}");
    let mut w = vec![1, 2, 3];
    let mut more = vec![4, 5];
    w.append(&mut more);
    println!("{w:?} {more:?} {} {}", w.capacity(), more.capacity());
    w.reserve(10);
    println!("{}", w.capacity());
    w.shrink_to_fit();
    w.reserve_exact(2);
    println!("{}", w.capacity());
    w.sort_by(|a, b| b.cmp(a));
    println!("{w:?} {} {:?} {:?}", w.contains(&3), &w[1..3], w.iter().max());
    for x in &mut w {
        *x *= 2;
    }
    w[0] += 1;
    let total: i32 = w.iter().sum();
    println!("{total} {}", w.len());
    let c = w.clone();
    println!("{} {} {}", c == w, c.capacity(), c < vec![100]);
    println!("{:?} {:?}", vec![0u8; 3], Vec::<char>::default());
    let filled = vec![String::from("ab"); 3];
    println!("{filled:?} {}", filled.capacity());

    let mut noisy = vec![Noisy(1), Noisy(2), Noisy(3), Noisy(4)];
    noisy.truncate(3);
    println!("truncated");
    let second = noisy.remove(1);
    println!("removed {}", second.0);
    drop(second);
    noisy.push(Noisy(5));
    for n in noisy {
        if n.0 == 3 {
            println!("stop");
            break;
        }
    }
    let kept = vec![Noisy(6), Noisy(7)];
    let mut many = vec![Noisy(8), Noisy(9)];
    many.clear();
    println!("cleared {}", many.capacity());
    let _ = kept;
    println!("end");
}
