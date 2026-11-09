macro_rules! empty { () => {} }
macro_rules! emit { ($($tokens:tt)*) => { $($tokens)* } }
macro_rules! choose {
    () => {
        #[cfg(all())]
        emit! { 42u32 }
        #[cfg(any())]
        emit! { missing }
        empty! {}
    }
}

fn chosen() -> u32 { choose! {} }
fn block() -> u32 { { 42u32 } empty! {} }
fn conditional(flag: bool) -> u32 { if flag { 42 } else { 0 } empty! {} }
fn ordered(log: &mut Vec<u32>) -> u32 {
    emit! { { log.push(1); } }
    emit! { let value = { log.push(2); 40 }; value + 2 }
    empty! {}
}
fn main() {
    let mut log = Vec::new();
    assert_eq!(chosen(), 42);
    assert_eq!(block(), 42);
    assert_eq!(conditional(true), 42);
    assert_eq!(ordered(&mut log), 42);
    assert_eq!(log, vec![1, 2]);
    println!("{} {:?}", chosen(), log);
}
