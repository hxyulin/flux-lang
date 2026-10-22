//! `collect` into `Vec` and `String`, and `extend`, with the capacities
//! rustc's specializations give: exact for iterators whose length is
//! known (`TrustedLen`: ranges, slices, `map`, `zip`, `chain`, `rev`,
//! `enumerate`, `copied`, `skip` and `step_by` of slices and ranges),
//! the first item then the lower bound otherwise (`filter`, `flatten`,
//! `chars`); `String` from `char`s, `&str`s and `String`s; `Option` and
//! `Result` of collections.

fn main() {
    let v: Vec<i32> = (0..10).map(|x| x * 2).collect();
    println!("{v:?} {}", v.capacity());
    // An owning iterator remains trusted through adapters, including
    // after consuming from either end. Changing element size prevents
    // rustc's separate allocation-reuse specialization from applying.
    let mut owned = vec![1u8, 2, 3].into_iter();
    let _ = owned.next();
    let _ = owned.next_back();
    let text: Vec<String> = owned.map(|x| x.to_string()).collect();
    println!("{text:?} {}", text.capacity());
    let f: Vec<i32> = v.iter().filter(|&&x| x > 5).copied().collect();
    println!("{f:?} {}", f.capacity());
    let e: Vec<(usize, char)> = "héllo".chars().enumerate().collect();
    println!("{e:?} {}", e.capacity());
    let s: String = vec!['a', 'b', 'c'].into_iter().rev().collect();
    println!("{s} {}", s.capacity());
    let words: String = ["ab", "cd"].iter().copied().collect();
    println!("{words} {}", words.capacity());
    let joined: String = vec![String::from("x"), String::from("yz")].into_iter().collect();
    println!("{joined} {}", joined.capacity());
    let mut w = vec![1, 2];
    w.extend(3..6);
    w.extend(&[9, 9]);
    w.extend(v.iter().filter(|x| **x > 15));
    println!("{w:?} {}", w.capacity());
    let skipped: Vec<u8> = [1u8, 2, 3, 4].iter().skip(1).copied().collect();
    println!("{skipped:?} {}", skipped.capacity());
    let stepped: Vec<i32> = (0..20).step_by(3).collect();
    println!("{stepped:?} {}", stepped.capacity());
    let flat: Vec<i32> = vec![vec![1], vec![2, 3]].into_iter().flatten().collect();
    println!("{flat:?} {}", flat.capacity());
    let none: Vec<i32> = (0..0).filter(|_| true).collect();
    println!("{none:?} {}", none.capacity());
    let chained: Vec<i32> = (0..3).chain(7..9).collect();
    println!("{chained:?} {}", chained.capacity());
    let zipped: Vec<(i32, char)> = (1..4).zip("abc".chars()).collect();
    println!("{zipped:?} {}", zipped.capacity());
    let bytes: Vec<u8> = "hey".bytes().collect();
    println!("{bytes:?} {}", bytes.capacity());
    let ok: Result<Vec<i32>, &str> = ["1", "2"].iter().map(|s| Ok(s.len() as i32)).collect();
    println!("{ok:?}");
    let some: Option<Vec<u8>> = [Some(1u8), None].into_iter().collect();
    println!("{some:?}");
    let mut text = String::from("ab");
    text.extend(['c', 'd'].iter());
    text.extend(["ef", "g"]);
    println!("{text} {}", text.capacity());
}
