//! Executed by both compilers inside the restricted production library.
use crate::{Edition, SyntaxKind, lexer::LexedStr};
mod corpus;

fn check(text: &str, edition: Edition) -> u64 {
    let tokens = LexedStr::with_edition(text, edition);
    assert_eq!(tokens.text(), text);
    assert_eq!(tokens.is_empty(), text.is_empty());
    assert_eq!(tokens.kinds().len(), tokens.len());
    let mut hash = 0u64;
    let mut end = 0;
    for i in 0..tokens.len() {
        let range = tokens.token_range(i);
        assert_eq!(range.start(), end);
        assert!(range.end() > end);
        assert_eq!(tokens.token_start(i), range.start());
        let token = tokens.token_text(i);
        assert_eq!(token, &text[range.start() as usize..range.end() as usize]);
        let context = tokens.is_union_keyword(i);
        assert_eq!(
            context,
            tokens.kind(i) == SyntaxKind::IDENT && token == "union"
        );
        hash = hash.rotate_left(7) ^ tokens.kind(i) as u64;
        hash = hash.wrapping_mul(31) ^ range.start() as u64;
        hash = hash.wrapping_mul(31) ^ range.end() as u64 ^ context as u64;
        for byte in token.bytes() {
            hash = hash.wrapping_mul(131) ^ byte as u64;
        }
        end = range.end();
    }
    assert_eq!(end, text.len() as u32);
    assert_eq!(tokens.token_start(tokens.len()), end);
    for error in tokens.errors() {
        assert!(error.token < tokens.len());
        hash = hash.rotate_left(11) ^ error.token as u64;
        for byte in error.message.bytes() {
            hash = hash.wrapping_mul(131) ^ byte as u64;
        }
    }
    hash
}

pub fn run() {
    let cases = [
        "",
        "fn main() { let café = 1; let 变量 = café; }",
        "// λ\n/* outer /* inner */ tail */ union r#union gen r#gen",
        "\"escaped \\\" λ\" b\"a\\x42\" c\"nul\\0\" 'λ' b'x' 'life",
        "r###\"quote \"## still raw\"### br##\"bytes\"## cr#\"cstr\"#",
        "0xff_u8 0b102 1..2 1.5e-2f64 3e+ 0o9 123suffix",
        "/* unterminated /* nested */",
        "\"unterminated\\",
        "r###\"missing\"##",
        "b'",
        "'ab'",
        "\0 😀 \\ §",
        "union unionized r#union gen",
        "fn recovered() {} /* bad",
        "r# b# cr##\"",
        "\r\n\t\u{85}\u{200e}",
    ];
    let mut total = 0u64;
    let mut prefixes = 0;
    for edition in [Edition::E2021, Edition::E2024] {
        assert_eq!(
            LexedStr::with_edition("gen", edition).kind(0),
            if edition == Edition::E2021 {
                SyntaxKind::IDENT
            } else {
                SyntaxKind::GEN_KW
            }
        );
        assert!(!LexedStr::new("/*").errors().is_empty());
        assert!(LexedStr::new("fn main() {}").errors().is_empty());
        for text in cases {
            // Every UTF-8 boundary exercises malformed prefixes and recovery.
            for end in 0..=text.len() {
                if text.is_char_boundary(end) {
                    total = total.rotate_left(3) ^ check(&text[..end], edition);
                    prefixes += 1;
                }
            }
            println!("case {:?} {} {}", edition, text.len(), check(text, edition));
            // Compare exact records too; corpus digests keep large-file output bounded.
            let tokens = LexedStr::with_edition(text, edition);
            for i in 0..tokens.len() {
                println!(
                    "token {:?} {:?} {:?} {}",
                    tokens.kind(i),
                    tokens.token_range(i),
                    tokens.token_text(i),
                    tokens.is_union_keyword(i)
                );
            }
            for error in tokens.errors() {
                println!("error {} {:?}", error.token, error.message);
            }
        }
        for (i, text) in corpus::CORPUS.iter().enumerate() {
            let digest = check(text, edition);
            println!("corpus {:?} {} {} {}", edition, i, text.len(), digest);
            total ^= digest;
        }
    }
    println!("prefixes {} digest {}", prefixes, total);
}
