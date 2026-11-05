// process::abort ends the process with SIGABRT, after complete lines of
// buffered output and without running destructors.
struct Loud;
impl Drop for Loud {
    fn drop(&mut self) { println!("dropped"); }
}

fn main() {
    let _loud = Loud;
    println!("before abort");
    std::process::abort();
}
