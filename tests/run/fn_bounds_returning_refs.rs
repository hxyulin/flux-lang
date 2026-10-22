//! `Fn`-family bounds whose result is a reference with the lifetime
//! their parameter elides (`F: Fn(&T) -> &K`, `impl Fn(&T) -> &U`), or
//! one a `for<'a>` binder names (`for<'a> Fn(&'a str) -> &'a str`):
//! each call's result borrows what the call is given, closures and
//! functions that return their argument (or a part of it) meet them,
//! and so do bounds on `impl` blocks and `FnMut` ones.

struct Item {
    name: &'static str,
    qty: u32,
}

fn max_by_key_ref<T, K: Ord, F: Fn(&T) -> &K>(xs: &[T], f: F) -> Option<&T> {
    let mut best: Option<&T> = None;
    for x in xs {
        best = match best {
            Some(b) if f(b) >= f(x) => Some(b),
            _ => Some(x),
        };
    }
    best
}

fn first_field<'a, T, U>(xs: &'a [T], get: impl Fn(&T) -> &U) -> Option<&'a U> {
    xs.first().map(|x| get(x))
}

fn apply<F>(f: F, s: &str) -> usize
where
    F: for<'a> Fn(&'a str) -> &'a str,
{
    f(s).len()
}

fn apply_once<F>(f: F, s: &str) -> usize
where
    for<'a> F: FnOnce(&'a str) -> &'a str,
{
    f(s).len()
}

// A nested item may reuse its enclosing function's lifetime name.
fn nested<'a>(s: &'a str) -> usize {
    fn inner<F: for<'a> Fn(&'a str) -> &'a str>(f: F, s: &str) -> usize {
        f(s).len()
    }
    inner(|x| x, s)
}

fn tail(s: &str) -> &str {
    &s[1..]
}

fn longest<F: FnMut(&str) -> &str>(words: &[&str], mut key: F) -> usize {
    let mut n = 0;
    for w in words {
        let k = key(w);
        if k.len() > n {
            n = k.len();
        }
    }
    n
}

fn last(r: &[u8]) -> &u8 {
    &r[r.len() - 1]
}

struct Picker<F> {
    pick: F,
}

impl<F: Fn(&[u8]) -> &u8> Picker<F> {
    fn total(&self, rows: &[&[u8]]) -> u32 {
        rows.iter().map(|r| *(self.pick)(r) as u32).sum()
    }
}

fn main() {
    let items = [Item { name: "hammer", qty: 3 }, Item { name: "saw", qty: 7 }];
    let best = max_by_key_ref(&items, |i| &i.qty).map_or(0, |i| i.qty);
    let by_name = max_by_key_ref(&items, |i| &i.name).map_or(0, |i| i.qty);
    let name = first_field(&items, |i| &i.name).map_or(0, |n| n.len());
    let n = apply(|s| s.trim(), "  ab  ") + apply(tail, "xyz")
        + apply_once(|s| s.trim(), " once ") + nested("nested");
    let mut calls = 0;
    let words = ["apple", "fig", "banana"];
    let l = longest(&words, |w| {
        calls += 1;
        &w[..w.len() / 2]
    });
    let picker = Picker { pick: last };
    let rows: [&[u8]; 2] = [&[1, 2, 3], &[4, 5]];
    println!("{best} {by_name} {name} {n} {l} {calls} {}", picker.total(&rows));
}
