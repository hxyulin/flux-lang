#[allow(dead_code)]
#[path = "../examples/unicode.rs"]
mod generator;

#[test]
fn unicode_tables_match_selected_toolchain() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../library/core/src/char_properties.rs");
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        generator::generate()
    );
}
