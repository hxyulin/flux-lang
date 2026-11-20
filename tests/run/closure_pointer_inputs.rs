//! Joining capture-free closures preserves the lifetimes of pointer inputs.

enum Token<'src> {
    Word(&'src str),
    Other,
}

fn apply<'src>(token: Token<'src>, f: impl Fn(Token<'src>) -> usize) -> usize {
    f(token)
}

fn classify(token: Token<'_>, keyword: bool) -> usize {
    apply(
        token,
        match keyword {
            true => |token| match token {
                Token::Word(word) => word.len(),
                Token::Other => 0,
            },
            false => |token| match token {
                Token::Word(_) => 1,
                Token::Other => 0,
            },
        },
    )
}

fn main() {
    let word = String::from("three");
    println!(
        "{} {} {}",
        classify(Token::Word(&word), true),
        classify(Token::Word(&word), false),
        classify(Token::Other, false)
    );
}
