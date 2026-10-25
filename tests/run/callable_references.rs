fn twice<F: FnMut(usize) -> bool>(mut f: F) -> usize {
    f(3) as usize + f(7) as usize
}

fn once<F: FnOnce(usize) -> usize>(f: F) -> usize { f(9) }

fn shared<F: Fn(usize) -> usize>(f: F) -> usize { f(2) + f(5) }

struct Table { value: u64 }
impl Table { fn bucket(&mut self, index: usize) -> u64 { self.value += index as u64; self.value } }
fn dynamic(table: &mut Table, callback: &dyn Fn(&mut Table, usize) -> u64) -> u64 {
    callback(table, 3) + callback(table, 5)
}
fn mutable_dynamic(callback: &mut dyn FnMut(usize) -> bool) -> usize {
    callback(2) as usize + callback(7) as usize
}

fn main() {
    let mut total = 0;
    println!("inline {}", twice(&mut |value| { total += value; value > 4 }));
    let mut stateful = |value| { total += value; total };
    println!("once {}", once(&mut stateful));
    println!("once-again {}", once(&mut stateful));
    let add = |value| value + 13;
    println!("shared {} {} {}", shared(&add), once(&add), twice(&|value| value % 2 == 1));
    println!("total {total}");
    let mut change = |value| value + 1;
    println!("mutable-once {}", once(&mut change));
    let mut table = Table { value: 17 };
    let delta = 3u64;
    println!("dynamic {}", dynamic(&mut table, &|table, index| table.bucket(index) + delta));
    let mut calls = 0;
    println!("mutable-dynamic {}", mutable_dynamic(&mut |value| { calls += 1; value > 3 }));
    println!("calls {calls} table {}", table.value);
}
