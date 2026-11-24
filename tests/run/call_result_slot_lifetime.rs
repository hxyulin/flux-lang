enum Packet {
    Bytes([u8; 224]),
    Label(String),
}

#[inline(never)]
fn make_packet(seed: u8) -> Packet {
    if seed % 3 == 0 {
        Packet::Label(seed.to_string())
    } else {
        Packet::Bytes([seed; 224])
    }
}

#[inline(never)]
fn replace(input: &Packet) -> Packet {
    match input {
        Packet::Bytes(bytes) => make_packet(bytes[223] + 1),
        Packet::Label(label) => make_packet(label.parse::<u8>().unwrap() + 1),
    }
}

fn main() {
    let mut previous = make_packet(0);
    let mut sum = 0;
    for seed in 1..18 {
        match make_packet(seed) {
            Packet::Bytes(ref bytes) => {
                assert_eq!(bytes[0], seed);
                sum += usize::from(bytes[223]);
            }
            Packet::Label(ref label) => {
                let number = label.parse::<u8>().unwrap();
                assert_eq!(number, seed);
                sum += usize::from(number);
            }
        }

        previous = replace(&previous);
        match &previous {
            Packet::Bytes(bytes) => assert_eq!(bytes[223], seed),
            Packet::Label(label) => assert_eq!(label.parse::<u8>().unwrap(), seed),
        }
    }
    assert_eq!(sum, 153);
    println!("{sum}");
}
