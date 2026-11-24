#[repr(i16)]
enum Token {
    A = -4,
    B(u8),
    C { value: u8 },
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Rest(u8),
}

#[inline(never)]
fn classify(token: &&Token, inspected: &mut Vec<u8>) -> u8 {
    match token {
        Token::A if true => 0,
        Token::B(_) if true => 1,
        Token::C { value: _ } => 2,
        Token::D => 3,
        Token::E => 4,
        Token::F => 5,
        Token::G => 6,
        Token::H => 7,
        Token::I => 8,
        Token::J => 9,
        Token::K => 10,
        Token::L => 11,
        Token::M => 12,
        Token::N => 13,
        Token::O => 14,
        Token::P => 15,
        Token::Rest(value)
            if {
                inspected.push(*value);
                *value == 3
            } =>
        {
            16
        }
        Token::Rest(7) => 17,
        Token::Rest(_) => 18,
        _ => 99,
    }
}

fn main() {
    let mut inspected = Vec::new();
    let tokens = [
        Token::A,
        Token::B(8),
        Token::C { value: 9 },
        Token::D,
        Token::E,
        Token::F,
        Token::G,
        Token::H,
        Token::I,
        Token::J,
        Token::K,
        Token::L,
        Token::M,
        Token::N,
        Token::O,
        Token::P,
        Token::Rest(3),
        Token::Rest(7),
        Token::Rest(8),
    ];
    for (expected, token) in tokens.iter().enumerate() {
        assert_eq!(classify(&token, &mut inspected), expected as u8);
    }
    assert_eq!(inspected, [3, 7, 8]);
    println!("dispatch matched");
}
