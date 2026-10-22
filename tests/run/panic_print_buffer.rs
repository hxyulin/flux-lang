//! stdout is line-buffered, as std's is: a panic, which aborts, loses
//! what `print!` left without a newline, while what ended a line was
//! written; stderr is not buffered.

fn step(n: u32) -> u32 {
    print!("step {} ", n);
    if n % 2 == 0 {
        println!("even");
    }
    eprint!("[{}]", n);
    n + 1
}

fn main() {
    let mut n = 0;
    while n < 5 {
        n = step(n);
    }
    eprintln!();
    let r: Result<u32, (i32, &str)> = Err((-1, "negative"));
    println!("before");
    print!("lost");
    let v = r.unwrap();
    println!("{}", v);
}
