//! Selected upstream Keel assertions and executable code-generation cases.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "support/backend_graph.rs"]
mod backend_graph;
mod support;

use flux_syntax::ast::{self, AstNode};
use serde_json::{Value, json};
use std::path::Path;

const PROGRAMS: &[&str] = &[
    "loop-none",
    "loop-full",
    "calls-none",
    "calls-full",
    "memory-none",
    "memory-full",
    "division-none",
    "division-full",
    "indirect-none",
    "indirect-full",
];

#[test]
#[ignore = "Flux-built backend executes upstream assertions and generated programs; run explicitly"]
fn flux_compiled_keel_reduced_suite() {
    backend_graph::run_suite(backend_graph::Suite {
        test: "flux_compiled_keel_reduced_suite",
        output: "keel-reduced-suite",
        harness: "crates/flux-cli/tests/keel_reduced_suite.rs",
        fixture: "crates/flux-cli/tests/fixtures/keel_reduced_suite/consumer.rs.txt",
        prepare: Some(prepare),
        default_scope: Some("full"),
        programs: PROGRAMS,
    });
}

fn prepare(root: &Path, packages: &[Value]) -> backend_graph::Prepared {
    let backend = packages
        .iter()
        .find(|p| p["package"] == "keel-codegen")
        .unwrap();
    let checkout = Path::new(backend["root"].as_str().unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let selection_path =
        repo.join("crates/flux-cli/tests/fixtures/keel_reduced_suite/selection.json");
    let selection: Vec<Value> =
        serde_json::from_slice(&std::fs::read(&selection_path).unwrap()).unwrap();
    let mut inputs = vec![selection_path];
    let mut declarations = String::new();
    let mut calls = String::new();
    let mut provenance = Vec::new();
    let mut count = 0;
    for group in selection {
        let name = group["module"].as_str().unwrap();
        let source_path = checkout.join(group["path"].as_str().unwrap());
        let source = std::fs::read_to_string(&source_path).unwrap();
        let parse = flux_syntax::parse_source_file(&source);
        assert!(
            parse.errors().is_empty(),
            "selected test syntax: {} {:?}",
            source_path.display(),
            parse.errors()
        );
        let wanted: Vec<&str> = group["functions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let tests: Vec<&str> = group["tests"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let mut found = Vec::new();
        let mut module = String::new();
        for node in parse.root().children() {
            if let Some(item) = ast::Use::cast(node) {
                let text = item.syntax().text();
                if !text.contains("support::") {
                    module.push_str(text);
                    module.push('\n');
                }
            } else if let Some(item) = ast::Const::cast(node) {
                module.push_str(item.syntax().text());
                module.push('\n');
            } else if let Some(function) = ast::Fn::cast(node) {
                let function_name = function.name().unwrap().syntax().text();
                if !wanted.contains(&function_name) {
                    continue;
                }
                let original = function.syntax().text();
                // Preserve every function body and assertion. Only the host test
                // registration attribute is removed for an ordinary binary.
                let copied = original.replace("#[test]", "");
                assert!(copied.contains(function.body().unwrap().syntax().text()));
                module.push_str(&copied);
                module.push('\n');
                provenance.push(json!({"source":source_path,"name":function_name,"range":format!("{:?}",function.syntax().text_range()),"test":tests.contains(&function_name),"adaptation":"remove #[test] registration only; body unchanged"}));
                found.push(function_name.to_owned());
            }
        }
        assert_eq!(
            found.len(),
            wanted.len(),
            "selection mismatch in {}: {found:?}",
            source_path.display()
        );
        module.push_str("pub fn run() {\n");
        for test in tests {
            assert!(wanted.contains(&test));
            module.push_str(&format!(
                "{test}(); println!(\"upstream {name}::{test} ok\");\n"
            ));
            count += 1;
        }
        module.push_str("}\n");
        let adapter = root.join(format!("selected_{name}.rs"));
        std::fs::write(&adapter, module).unwrap();
        declarations.push_str(&format!("#[path={:?}] mod {name};\n", adapter));
        calls.push_str(&format!("{name}::run();\n"));
        inputs.push(source_path);
        inputs.push(adapter);
    }
    std::fs::write(
        root.join("selected-tests.json"),
        serde_json::to_vec_pretty(&provenance).unwrap(),
    )
    .unwrap();
    let fixture = repo.join("crates/flux-cli/tests/fixtures/keel_reduced_suite/consumer.rs.txt");
    let consumer = std::fs::read_to_string(fixture)
        .unwrap()
        .replace("__SELECTED_TEST_MODULES__", &declarations)
        .replace("__RUN_SELECTED_TESTS__", &calls)
        .replace("__UPSTREAM_TEST_COUNT__", &count.to_string());
    backend_graph::Prepared { consumer, inputs }
}
