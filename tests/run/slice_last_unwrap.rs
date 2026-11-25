#[inline(never)]
fn last_state(states: &[i16]) -> i16 {
    *states.last().unwrap()
}

#[inline(never)]
fn last_record(records: &[[u32; 3]]) -> [u32; 3] {
    *records.last().unwrap()
}

#[inline(never)]
fn last_unit(units: &[()]) -> &() {
    units.last().unwrap()
}

fn main() {
    if std::env::args().nth(1).is_some() {
        println!("{}", last_state(&[]));
        return;
    }
    let mut states = vec![-120i16, 309, -901];
    println!("{}", last_state(&states));
    states.truncate(1);
    println!("{}", last_state(&states));

    let records = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
    println!("{:?}", last_record(&records[..2]));

    let units = [(); 5];
    println!("{}", std::ptr::eq(last_unit(&units), &units[4]));
}
