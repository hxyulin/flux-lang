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
