use flux_syntax::{Edition, SyntaxKind, parse_source_text_with_edition};

#[test]
fn gen_is_an_identifier_only_in_2021_and_trees_stay_lossless() {
    let source = include_str!("../../flux-cli/tests/fixtures/edition2021/lib.rs.txt");
    let old = parse_source_text_with_edition(source.into(), Edition::E2021);
    assert!(old.errors().is_empty(), "{}", old.debug_dump());
    assert_eq!(old.root().to_string(), source);
    old.tree().validate().unwrap();
    assert!(
        !parse_source_text_with_edition(source.into(), Edition::E2024)
            .errors()
            .is_empty()
    );
    for (text, expected) in [("gen", SyntaxKind::IDENT), ("r#gen", SyntaxKind::IDENT)] {
        assert_eq!(
            flux_syntax::LexedStr::with_edition(text, Edition::E2021).kind(0),
            expected
        );
    }
}

#[test]
fn rust_2018_tokens_and_legacy_expression_fragments() {
    let source = "fn gen() { let _ = 1; }";
    for edition in [Edition::E2018, Edition::E2021] {
        let parsed = parse_source_text_with_edition(source.into(), edition);
        assert!(parsed.errors().is_empty(), "{}", parsed.debug_dump());
        assert_eq!(parsed.root().to_string(), source);
        parsed.tree().validate().unwrap();
    }
    for (text, legacy, modern) in [
        ("const { 1 }", false, true),
        ("_", false, true),
        ("(const {1})", true, true),
    ] {
        let lexed = flux_syntax::LexedStr::new(text);
        let input = flux_syntax::parser::Input::from_lexed(&lexed);
        assert_eq!(
            flux_syntax::parser::fragment_len(&input, 0, flux_syntax::parser::Fragment::Expr2021)
                .is_some(),
            legacy
        );
        assert_eq!(
            flux_syntax::parser::fragment_len(&input, 0, flux_syntax::parser::Fragment::Expr)
                .is_some(),
            modern
        );
    }
}

#[test]
fn rust_2015_keywords_and_contextual_dyn_preserve_lexical_identity() {
    use flux_syntax::ast::{self, AstNode};
    for word in ["async", "await", "dyn", "try", "gen"] {
        assert_eq!(
            flux_syntax::LexedStr::with_edition(word, Edition::E2015).kind(0),
            SyntaxKind::IDENT
        );
    }
    let source = "mod dyn{pub struct T;}fn async(x:&dyn Trait,y:dyn::T){let await=1;let try=2;}";
    let parsed = parse_source_text_with_edition(source.into(), Edition::E2015);
    assert!(parsed.errors().is_empty(), "{}", parsed.debug_dump());
    assert_eq!(parsed.root().to_string(), source);
    parsed.tree().validate().unwrap();
    let object = parsed
        .root()
        .descendants()
        .find_map(ast::DynTraitType::cast)
        .unwrap();
    let keyword = object
        .syntax()
        .tokens()
        .find(|t| t.kind() == SyntaxKind::DYN_KW)
        .unwrap();
    assert_eq!(keyword.lexical_kind(), SyntaxKind::IDENT);
    let modern = parse_source_text_with_edition("fn f(x:&dyn Trait){}".into(), Edition::E2018);
    let keyword = modern
        .root()
        .tokens()
        .find(|t| t.kind() == SyntaxKind::DYN_KW)
        .unwrap();
    assert_eq!(keyword.lexical_kind(), SyntaxKind::DYN_KW);
    let raw = parse_source_text_with_edition("fn f(x:&r#dyn){}".into(), Edition::E2015);
    assert!(raw.errors().is_empty());
    assert!(
        !raw.root()
            .descendants()
            .any(|n| n.kind() == SyntaxKind::DYN_TRAIT_TYPE)
    );
}
