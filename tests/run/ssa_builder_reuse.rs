#[inline(never)]
fn branch_loop(mut x: i32, limit: i32) -> i32 {
    let mut total = 0;
    let mut i = 0;
    while i < limit {
        if x & 1 == 0 {
            total += x;
            x = x / 2 + 3;
        } else {
            total -= x;
            x = x * 2 - 1;
        }
        if total > 100 {
            break;
        }
        i += 1;
    }
    total + x + i
}

#[inline(never)]
fn small(x: u8) -> u64 {
    x as u64 + 1
}

#[inline(never)]
fn nested(n: u32) -> (u32, u32) {
    let mut left = 0;
    let mut right = n;
    let mut i = 0;
    while i < n {
        let mut j = 0;
        while j < i {
            if j & 1 == 0 {
                left += j;
            } else {
                right += i;
            }
            j += 1;
        }
        i += 1;
    }
    (left, right)
}

fn main() {
    let pair = nested(8);
    println!(
        "{} {} {} {} {}",
        branch_loop(8, 6),
        small(41),
        pair.0,
        pair.1,
        branch_loop(3, 5)
    );
}
