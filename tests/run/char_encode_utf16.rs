const UNITS: [u16; 2] = {
    let mut units = [0; 2];
    '\u{10000}'.encode_utf16(&mut units);
    units
};

fn main() {
    println!("{UNITS:?}");
    let text = "\0\u{D7FF}\u{E000}\u{FFFF}\u{10000}\u{10FFFF}";
    for scalar in text.chars() {
        let mut buffer = [123; 3];
        let len = {
            let written = scalar.encode_utf16(&mut buffer);
            println!("{written:?}");
            written.len()
        };
        println!("{:?}", &buffer[len..]);
    }
    println!("{:?}", text.encode_utf16().collect::<Vec<_>>());
}
