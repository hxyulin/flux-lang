// Appended inside the unchanged syntax_kind module by the host harness.
// Access its keyword spellings without duplicating the production table.
pub(crate) fn run_selfhost_checks() {
    assert_eq!(core::mem::size_of::<SyntaxKind>(), 2);
    for (text, kind) in KEYWORDS {
        let actual = SyntaxKind::from_keyword(core::hint::black_box(text));
        assert_eq!(actual, Some(kind));
        assert!(kind.is_keyword());
        assert!(!kind.is_punct());
        println!("keyword {text}: {actual:?} {}", kind.is_reserved_keyword());
    }
    for text in ["", "a", "As", "self_", "continues", "fn\0", "\0", "matches", "_", "é", "long_identifier"] {
        assert_eq!(SyntaxKind::from_keyword(core::hint::black_box(text)), None);
    }

    for character in [';', ',', '(', ')', '{', '}', '[', ']', '<', '>', '@', '#', '~', '?', '$', '&', '|', '+', '*', '/', '^', '%', '_', '.', ':', '=', '!', '-', 'a', 'é'] {
        let kind = SyntaxKind::from_char(core::hint::black_box(character));
        println!("character {character}: {kind:?}");
    }

    // The actual exported T! macro includes terminal literal `$`, and
    // composite_parts returns arrays promoted by the production method.
    assert_eq!(crate::T![$], SyntaxKind::DOLLAR);
    let composites = [
        crate::T![..], crate::T![...], crate::T![..=], crate::T![::],
        crate::T![==], crate::T![=>], crate::T![!=], crate::T![->],
        crate::T![<=], crate::T![>=], crate::T![&&], crate::T![||],
        crate::T![<<], crate::T![>>], crate::T![+=], crate::T![-=],
        crate::T![*=], crate::T![/=], crate::T![%=], crate::T![^=],
        crate::T![&=], crate::T![|=], crate::T![<<=], crate::T![>>=],
    ];
    let saved = SyntaxKind::DOT2EQ.composite_parts().unwrap();
    for kind in composites {
        let parts = core::hint::black_box(kind).composite_parts().unwrap();
        assert!(kind.is_punct());
        assert!(parts.len() == 2 || parts.len() == 3);
        println!("composite {kind:?}: {parts:?}");
    }
    assert_eq!(saved, &[SyntaxKind::DOT, SyntaxKind::DOT, SyntaxKind::EQ]);
    assert_eq!(SyntaxKind::IDENT.composite_parts(), None);
    assert!(SyntaxKind::WHITESPACE.is_trivia());
    assert!(SyntaxKind::COMMENT.is_trivia());
    assert!(SyntaxKind::INT_NUMBER.is_literal());
    assert!(!SyntaxKind::IDENT.is_literal());
    assert!(SyntaxKind::FN.takes_leading_comments());
    assert!(!SyntaxKind::IDENT.takes_leading_comments());
}
