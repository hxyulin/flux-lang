//! Opt-in Flux compilation of dependency-free procedural macro hosts.

use std::collections::HashSet;
use std::path::Path;

use flux_hir::{CrateCfg, ProcMacro, ProcMacroKind};
use flux_intern::Symbol;
use flux_syntax::Edition;
use flux_syntax::ast::{self, AstNode};

use crate::build::{CompileRequest, SourceInput};
use crate::proc_macro::local::{self, Export};

pub(crate) fn build_host(
    source: &Path,
    name: &str,
    edition: Edition,
    output: &Path,
) -> Result<Vec<ProcMacro>, String> {
    std::fs::create_dir_all(output)
        .map_err(|e| format!("cannot create macro host directory: {e}"))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let cfg = native_cfg();
    let prepared = local::prepare_source_with_cfg(&source, &output, &cfg)?;
    validate_exports(&prepared.text, &prepared.exports, &cfg)?;
    let runtime = output.join("runtime");
    std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    flux_proc_macro::write_sources(&runtime).map_err(|e| e.to_string())?;
    let mut arms = String::new();
    for export in &prepared.exports {
        let invocation = if export.kind == ProcMacroKind::Attribute {
            format!(
                "{{ let entry: fn(__flux_proc_macro::TokenStream, __flux_proc_macro::TokenStream) -> __flux_proc_macro::TokenStream = crate::{}; entry(attribute, input) }}",
                export.function
            )
        } else {
            format!(
                "{{ let entry: fn(__flux_proc_macro::TokenStream) -> __flux_proc_macro::TokenStream = crate::{}; entry(input) }}",
                export.function
            )
        };
        arms.push_str(&format!("{:?} => {invocation},\n", export.name));
    }
    let runner = format!(
        r#"
{}
extern crate proc_macro as __flux_proc_macro;
fn __flux_macro_run() -> Result<(), String> {{
    let name = std::env::var("FLUX_PROC_MACRO_NAME").map_err(|e| e.to_string())?;
    let attribute_path = std::env::var("FLUX_PROC_MACRO_ATTRIBUTE").map_err(|e| e.to_string())?;
    let input_path = std::env::var("FLUX_PROC_MACRO_INPUT").map_err(|e| e.to_string())?;
    let output_path = std::env::var("FLUX_PROC_MACRO_OUTPUT").map_err(|e| e.to_string())?;
    let attribute_bytes = std::fs::read(attribute_path).map_err(|e| e.to_string())?;
    let input_bytes = std::fs::read(input_path).map_err(|e| e.to_string())?;
    let attribute = __flux_proc_macro::TokenStream::from_wire(&attribute_bytes).map_err(|e| e.to_string())?;
    let input = __flux_proc_macro::TokenStream::from_wire(&input_bytes).map_err(|e| e.to_string())?;
    let output: __flux_proc_macro::TokenStream = match name.as_str() {{
        {arms}
        _ => return Err("unknown procedural macro".to_owned()),
    }};
    std::fs::write(output_path, output.to_wire()).map_err(|e| e.to_string())
}}
fn main() {{ __flux_macro_run().expect("procedural macro host failed"); }}
"#,
        prepared.text
    );
    std::fs::write(&prepared.path, &runner).map_err(|e| e.to_string())?;
    let sources = [
        SourceInput {
            edition: Edition::E2024,
            file: runtime.join("lib.rs"),
            name: "proc_macro".into(),
            dependencies: Vec::new(),
            host_macros: None,
            compilation_dir: None,
            environment: None,
            cfg: CrateCfg::default(),
        },
        SourceInput {
            edition,
            file: source.clone(),
            name: name.into(),
            dependencies: vec![("proc_macro".into(), 0)],
            host_macros: None,
            compilation_dir: None,
            environment: None,
            cfg: CrateCfg {
                names: vec![Symbol::intern("proc_macro")],
                values: Vec::new(),
            },
        },
    ];
    let executable = output.join("host");
    if !crate::build::compile_generated(
        CompileRequest {
            file: &source,
            crate_name: name,
            output: &executable,
            options: crate::BuildOptions {
                optimize: false,
                timings: false,
            },
            sources: &sources,
        },
        &runner,
    )? {
        return Err("cannot compile procedural macro host with flux".into());
    }
    Ok(local::descriptors(&executable, prepared.exports))
}

fn native_cfg() -> HashSet<String> {
    let mut cfg = HashSet::from([
        "proc_macro".into(),
        "debug_assertions".into(),
        "unix".into(),
    ]);
    for (key, value) in [
        ("target_os", std::env::consts::OS),
        ("target_arch", std::env::consts::ARCH),
        ("target_family", "unix"),
        ("target_pointer_width", "64"),
        ("target_endian", "little"),
        ("panic", "abort"),
        (
            "target_vendor",
            if cfg!(target_os = "macos") {
                "apple"
            } else {
                "unknown"
            },
        ),
    ] {
        cfg.insert(format!("{key}={value:?}"));
    }
    for feature in match std::env::consts::ARCH {
        "aarch64" => &["neon"][..],
        "x86_64" => &["fxsr", "sse", "sse2"][..],
        _ => &[],
    } {
        cfg.insert(format!("target_feature={feature:?}"));
    }
    cfg
}

// Native hosts cannot use rustc to enforce proc-macro export restrictions.
// Reject unsupported entry shapes rather than accepting an invalid Rust crate.
fn validate_exports(text: &str, exports: &[Export], cfg: &HashSet<String>) -> Result<(), String> {
    let parse = flux_syntax::parse_source_file(text);
    let mut names = HashSet::new();
    let mut functions = HashSet::new();
    for export in exports {
        if !names.insert(&export.name) {
            return Err("duplicate procedural macro export".into());
        }
        if !functions.insert(&export.function) {
            return Err("duplicate procedural macro entry attribute".into());
        }
    }
    for node in parse.root().children() {
        if !local::item_enabled(node, cfg)? {
            continue;
        }
        let public = node
            .children()
            .find_map(ast::Visibility::cast)
            .is_some_and(|vis| {
                vis.syntax()
                    .tokens()
                    .filter(|t| !t.kind().is_trivia())
                    .count()
                    == 1
            });
        let entry = ast::Fn::cast(node)
            .and_then(|function| function.name().map(|name| (function, name.text())));
        let export =
            entry.and_then(|(_, name)| exports.iter().find(|export| export.function == name));
        if public && export.is_none() {
            return Err("proc-macro crates cannot export non-macro items".into());
        }
        if export.is_some() {
            let function = ast::Fn::cast(node).ok_or("expected a macro function")?;
            if !public {
                return Err("procedural macro functions must be public".into());
            }
            if function.generic_param_list().is_some()
                || function.where_clause().is_some()
                || function.async_token().is_some()
                || function.unsafe_token().is_some()
                || function.abi().is_some()
                || function.const_token().is_some()
            {
                return Err(
                    "qualified or generic native macro entries are not supported yet".into(),
                );
            }
            if function.param_list().is_some_and(|params| {
                params
                    .syntax()
                    .descendants()
                    .any(|node| node.kind() == flux_syntax::SyntaxKind::IMPL_TRAIT_TYPE)
            }) {
                return Err(
                    "impl trait parameters in procedural macro entries are not supported yet"
                        .into(),
                );
            }
        }
        for attr in node.children().filter_map(ast::Attr::cast) {
            if attr
                .meta()
                .and_then(|meta| meta.path())
                .is_some_and(|path| path.syntax().text() == "macro_export")
            {
                return Err("macro_rules exports in proc-macro crates are not supported".into());
            }
        }
    }
    Ok(())
}
