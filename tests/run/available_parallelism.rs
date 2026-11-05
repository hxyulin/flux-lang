// The parallelism estimate is positive, stable, and matches the system tool.
use std::process::Command;

fn main() {
    let count = std::thread::available_parallelism().unwrap();
    assert_eq!(count, std::thread::available_parallelism().unwrap());
    let output = if cfg!(target_os = "macos") {
        Command::new("/usr/bin/getconf").arg("_NPROCESSORS_ONLN").output()
    } else {
        Command::new("nproc").output()
    }
    .unwrap();
    let expected: usize = String::from_utf8(output.stdout).unwrap().trim().parse().unwrap();
    println!("positive: {}", count.get() > 0);
    println!("matches the system: {}", count.get() == expected);
}
