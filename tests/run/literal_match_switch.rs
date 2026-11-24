fn classify(value: i16, log: &mut Vec<i16>) -> i16 {
    match value {
        0 => 0,
        1 => 3,
        2 => 6,
        3 => 9,
        4 => 12,
        5 => 15,
        6 => 18,
        7 => 21,
        8 => 24,
        9 => 27,
        10 => 30,
        11 => 33,
        12 => 36,
        13 => 39,
        14 => 42,
        15 => 45,
        16 => 48,
        17 => 51,
        18 => 54,
        19 => 57,
        20 if {
            log.push(value);
            false
        } =>
        {
            100
        }
        20 => 101,
        -1 => 102,
        19 => 103,
        _ => 104,
    }
}

fn main() {
    let mut log = Vec::new();
    for input in [-1, 0, 9, 19, 20, 21] {
        println!("{}", classify(input, &mut log));
    }
    println!("{:?}", log);
}
