fn sum(v: &[i32]) -> i32 {
    let mut total = 0;
    for x in v {
        total += *x;
    }
    total
}

fn max(v: &[i32]) -> Option<&i32> {
    let mut best = v.first()?;
    for x in v.iter() {
        if *x > *best {
            best = x;
        }
    }
    Some(best)
}

fn main() {
    let a = [3, 1, 4, 1, 5];
    let s: &[i32] = &a;
    let total = sum(s);
    let m = *max(s).unwrap();
    let evens = s.iter().filter(|x| **x % 2 == 0).count();
    let doubled: i32 = s.iter().map(|x| *x * 2).sum();
    let last = *s.last().unwrap_or(&0);
    let back = *s.iter().rev().next().unwrap();
    let got = match s.get(9) {
        Some(x) => *x,
        None => -1,
    };
    let n = total + m + evens as i32 + doubled + last + back + got;
    if n == 0 {
        loop {}
    }
}
