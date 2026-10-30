use flux_syntax::ast::{AstNode, Item};

/// Assemble calls from the current generated declarations, rather than freezing
/// a second AST inventory or depending on a research script/temp artifact.
pub fn comprehensive_consumer(generated: &str) -> String {
    let parsed = flux_syntax::parse_source_file(generated);
    assert!(parsed.errors().is_empty());
    let mut calls = String::new();
    let (mut structs, mut enums, mut nodes, mut accessors, mut conversions) = (0, 0, 0, 0, 0);
    for item in parsed.source_file().items() {
        match item {
            Item::Struct(_) => structs += 1,
            Item::Enum(_) => enums += 1,
            Item::Impl(implementation) => {
                let self_ty = implementation.self_ty().unwrap();
                let self_text = self_ty.syntax().text();
                let name = self_text.split('<').next().unwrap();
                if let Some(trait_ty) = implementation.trait_ty() {
                    let text: String = trait_ty.syntax().text().split_whitespace().collect();
                    if text.starts_with("AstNode<") {
                        nodes += 1;
                        calls.push_str(&format!(
                            "if let Some(n) = ast::{name}::cast(root) {{ round(n, root); }}\n"
                        ));
                    } else if let Some(source) = text.strip_prefix("From<") {
                        let source = source.split('<').next().unwrap();
                        conversions += 1;
                        calls.push_str(&format!(
                            "if let Some(n) = ast::{source}::cast(root) {{ let converted: ast::{name} = n.into(); assert_eq!(converted.syntax(), root); }}\n"
                        ));
                    } else {
                        panic!("unexpected generated trait implementation: {text}");
                    }
                } else {
                    calls.push_str(&format!("if let Some(n) = ast::{name}::cast(root) {{\n"));
                    for item in implementation.assoc_item_list().unwrap().items() {
                        let Item::Fn(function) = item else {
                            panic!("unexpected generated associated item");
                        };
                        let method = function.name().unwrap().text();
                        accessors += 1;
                        let ret = function.ret_type().unwrap().syntax().text();
                        if ret.contains("AstChildren") {
                            calls.push_str(&format!(
                                "println!(\"{name}::{method} {{}}\", n.{method}().count());\n"
                            ));
                        } else {
                            calls.push_str(&format!(
                                "println!(\"{name}::{method} {{:?}}\", n.{method}());\n"
                            ));
                        }
                    }
                    calls.push_str("}\n");
                }
            }
            _ => {}
        }
    }
    // Review coverage explicitly if sourcegen changes its inventory. These are
    // 405 trait interface calls, 536 accessors and 82 conversion call sites.
    assert_eq!(
        (structs, enums, nodes, accessors, conversions),
        (128, 7, 135, 536, 82)
    );
    let template = include_str!("../fixtures/typed_ast_readiness/consumer.rs.txt");
    assert_eq!(template.matches("// GENERATED_INTERFACES").count(), 1);
    let mut inputs = String::new();
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for category in ["ok", "err"] {
        let mut paths: Vec<_> = std::fs::read_dir(
            repository.join(format!("crates/flux-syntax/tests/data/parser/{category}")),
        )
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(path).unwrap();
            inputs.push_str(&format!("{text:?},\n"));
        }
    }
    assert_eq!(template.matches("// PARSER_INPUTS").count(), 1);
    template
        .replace("// GENERATED_INTERFACES", &calls)
        .replace("// PARSER_INPUTS", &inputs)
}
