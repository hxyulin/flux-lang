//! `String`: `new`, `with_capacity`, `from`, `to_string` and
//! `to_owned` of text, `push`, `push_str`, `+` and `+=` with `&str`,
//! `pop`, `insert`, `insert_str`, `remove`, `truncate`, `clear`,
//! `split_off`, capacities as rustc grows them, `str` methods through
//! `Deref`, comparisons with `&str`, `String` and `str`, `Clone`,
//! `Display`, `Debug`, `from_utf8`, `into_bytes` and `repeat`.

fn shout(s: &str) -> usize {
    s.len()
}

fn main() {
    let mut s = String::new();
    println!("{} {}", s.len(), s.capacity());
    for (i, word) in ["a", "bc", "def", "ghij", "klmno"].iter().enumerate() {
        s.push_str(word);
        s.push(if i % 2 == 0 { '-' } else { 'é' });
        println!("{s} {} {}", s.len(), s.capacity());
    }
    let t = String::from("hello");
    let u = t.clone() + ", " + "world";
    let mut v = "x".to_string();
    v += "yz";
    v.push('🦀');
    println!("{u} {} {v} {} {}", t.capacity(), v.len(), v.capacity());
    println!("{:?} {}", v.pop(), v);
    v.insert(1, 'Q');
    v.insert_str(0, ">>");
    println!("{v} {}", v.remove(2));
    println!("{} {}", shout(&u), u.to_uppercase_ascii_len());
    let mut w = String::with_capacity(3);
    w.push_str("abcdef");
    println!("{w} {}", w.capacity());
    let tail = w.split_off(4);
    w.truncate(2);
    println!("{w} {tail} {}", w.capacity());
    w.clear();
    println!("[{w}] {} {}", w.is_empty(), w.capacity());
    let owned: String = "own".to_owned();
    println!("{owned:?} {}", owned == "own");
    println!("{} {} {}", u.starts_with("hel"), u.contains("lo, w"), u.find('w').unwrap());
    let words: Vec<&str> = u.split(", ").fold(Vec::new(), |mut acc, w| {
        acc.push(w);
        acc
    });
    println!("{words:?}");
    let bytes = u.clone().into_bytes();
    println!("{} {:?}", bytes.len(), String::from_utf8(bytes).unwrap());
    println!("{}", String::from_utf8(vec![0xff, 0xfe]).is_err());
    println!("{}", "ab".repeat(3));
    println!("{} {}", String::from("b") > String::from("a"), String::from('c'));
    println!("{}", u.chars().rev().filter(|c| c.is_ascii_alphabetic()).count());
}

trait UpperLen {
    fn to_uppercase_ascii_len(&self) -> usize;
}

impl UpperLen for String {
    fn to_uppercase_ascii_len(&self) -> usize {
        self.bytes().filter(|b| b.is_ascii_lowercase()).count()
    }
}
