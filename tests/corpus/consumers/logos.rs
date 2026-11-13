use logos::Logos;
#[derive(Logos, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+")]
enum Token {
    #[token("let")]
    Let,
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |lexer| lexer.slice().to_owned())]
    Ident(String),
    #[regex("[0-9]+", |lexer| lexer.slice().parse::<u32>().unwrap())]
    Number(u32),
}
fn main() {
    let mut lexer=Token::lexer("let name 42 ?");
    assert_eq!(lexer.next(),Some(Ok(Token::Let)));
    assert_eq!(lexer.slice(),"let");
    assert_eq!(lexer.remainder()," name 42 ?");
    assert_eq!(lexer.next(),Some(Ok(Token::Ident("name".into()))));
    assert_eq!(lexer.span(),4..8);
    assert_eq!(lexer.next(),Some(Ok(Token::Number(42))));
    assert_eq!(lexer.slice(),"42");
    assert_eq!(lexer.next(),Some(Err(())));
    assert_eq!(lexer.next(),None);
    println!("tokens, slices, spans, callbacks and errors");
}
