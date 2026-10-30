use std::io::Write;
use std::process::{Command, Stdio};

fn main() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "IFS= read -r line; printf '%s' \"$line\"; printf 'pipe stderr' >&2; exit 7"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"pipe payload\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(output.stdout, b"pipe payload");
    assert_eq!(output.stderr, b"pipe stderr");
    println!("process pipes and exit status");
}
