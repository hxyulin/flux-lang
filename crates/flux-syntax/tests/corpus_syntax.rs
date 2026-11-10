//! Stable syntax forms reduced from the registry corpus.
use flux_syntax::{Edition, parse_source_text_with_edition};

fn compare(name: &str, source: &str, edition: Edition, accepted: bool) {
    let edition_name = match edition {
        Edition::E2015 => "2015",
        Edition::E2018 => "2018",
        Edition::E2021 => "2021",
        Edition::E2024 => "2024",
    };
    let dir =
        std::env::temp_dir().join(format!("flux-corpus-syntax-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args([
            "--edition",
            edition_name,
            "--crate-type",
            "lib",
            "--emit=metadata",
            "--out-dir",
        ])
        .arg(&dir)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        oracle.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
    let parsed = parse_source_text_with_edition(source.into(), edition);
    assert_eq!(
        parsed.errors().is_empty(),
        accepted,
        "flux {name}: {}",
        parsed.debug_dump()
    );
    assert_eq!(parsed.root().text(), source);
    parsed.tree().validate().unwrap();
}

#[test]
fn raw_lifetimes_follow_the_source_edition() {
    let source = "struct Raw<'r#mod>(&'r#mod u8);";
    compare("raw-2021", source, Edition::E2021, true);
    compare("raw-2024", source, Edition::E2024, true);
    compare("raw-2018", source, Edition::E2018, false);
    compare("raw-2015", source, Edition::E2015, false);
    compare(
        "raw-underscore",
        "struct Raw<'r#_>(&'r#_ u8);",
        Edition::E2024,
        false,
    );
}

#[test]
fn foreign_variadics_and_safe_items() {
    compare(
        "foreign",
        "type Callback = unsafe extern \"C\" fn(i32, ...) -> i32; unsafe extern \"C\" { fn variadic(first:i32,...); safe fn count()->u32; safe static COUNT:u32; }",
        Edition::E2024,
        true,
    );
    compare(
        "variadic-not-last",
        "type Callback = unsafe extern \"C\" fn(..., i32);",
        Edition::E2024,
        false,
    );
    compare(
        "safe-outside-extern",
        "safe fn outside() {}",
        Edition::E2024,
        false,
    );
    compare(
        "safe-is-a-name",
        "fn safe()->u32{1} fn use_safe(){let safe=safe();}",
        Edition::E2024,
        true,
    );
}

#[test]
fn parenthesized_dyn_bound() {
    compare(
        "parenthesized-dyn",
        "fn display(_: &dyn (std::fmt::Debug)) {}",
        Edition::E2024,
        true,
    );
}

#[test]
fn anonymous_trait_parameters_are_limited_to_rust_2015() {
    let source = "trait Legacy<T> { fn take(T); fn replace(&mut self, Self); }";
    compare("legacy-trait", source, Edition::E2015, true);
    compare("modern-trait", source, Edition::E2024, false);
    compare("legacy-free", "fn take(u32) {}", Edition::E2015, false);
    compare(
        "legacy-impl",
        "struct S; impl S { fn take(u32) {} }",
        Edition::E2015,
        false,
    );
    compare(
        "legacy-nested",
        "trait Legacy { fn outer(){fn nested(u32){}} }",
        Edition::E2015,
        false,
    );
}

#[test]
fn expansion_inputs_keep_edition_and_contextual_qualifiers() {
    use flux_syntax::{
        TextRange,
        parser::{Fragment, Input},
    };
    let source = "trait Legacy<T> {fn take(T);}";
    for (edition, accepted) in [(Edition::E2015, true), (Edition::E2024, false)] {
        let parse = flux_syntax::parse_expansion_with_edition(
            source.into(),
            &[],
            TextRange::new(0, source.len() as u32),
            Fragment::Items,
            edition,
        );
        assert_eq!(
            parse.errors().is_empty(),
            accepted,
            "{edition:?}: {}",
            parse.debug_dump()
        );
    }
    let source = "safe fn count()->u32;";
    let parse = flux_syntax::parse_expansion_with_edition(
        source.into(),
        &[],
        TextRange::new(0, source.len() as u32),
        Fragment::Items,
        Edition::E2024,
    );
    assert!(parse.errors().is_empty(), "{}", parse.debug_dump());
    let source = "unsafe extern \"C\" {safe fn count()->u32;}";
    let lexed = flux_syntax::LexedStr::with_edition(source, Edition::E2024);
    let mut input = Input::with_edition(Edition::E2024);
    for i in 0..lexed.len() {
        if lexed.kind(i).is_trivia() {
            continue;
        }
        input.push(lexed.kind(i), false);
        if lexed.token_text(i) == "safe" {
            input.set_last_safe_keyword();
        }
        if i + 1 < lexed.len() && !lexed.kind(i + 1).is_trivia() {
            input.set_last_joint();
        }
    }
    assert!(flux_syntax::parser::fragment_len(&input, 0, Fragment::Item).is_some());
}
