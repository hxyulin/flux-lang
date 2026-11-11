//! Markdown examples compiled with the package's real source dependency graph.
use crate::build::SourceInput;
use flux_syntax::ast::AstNode;
use std::path::Path;
use std::process::{Command, ExitCode};

#[derive(Default, Debug)]
struct Example {
    code: String,
    ignore: bool,
    no_run: bool,
    compile_fail: bool,
    should_panic: bool,
    line: usize,
    edition: Option<flux_syntax::Edition>,
}

fn markdown(docs: &str) -> Vec<Example> {
    let mut result = Vec::new();
    let mut active: Option<(String, Example)> = None;
    let mut foreign_fence: Option<String> = None;
    for (line, text) in docs.lines().enumerate() {
        let trimmed = text
            .trim_start()
            .strip_prefix("* ")
            .unwrap_or(text.trim_start());
        if let Some(fence) = &foreign_fence {
            if trimmed.starts_with(fence.as_str()) {
                foreign_fence = None;
            }
            continue;
        }
        if let Some((fence, example)) = &mut active {
            if trimmed.starts_with(fence.as_str()) {
                let (_, example) = active.take().unwrap();
                result.push(example);
                continue;
            }
            let code = trimmed
                .strip_prefix("# ")
                .unwrap_or(if trimmed == "#" { "" } else { text });
            example.code.push_str(if code.starts_with("##") {
                &code[1..]
            } else {
                code
            });
            example.code.push('\n');
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let mark = trimmed.chars().next().unwrap();
            let len = trimmed.chars().take_while(|&c| c == mark).count();
            let info = trimmed[len..].trim();
            let flags: Vec<_> = info
                .split([',', ' ', '\t'])
                .filter(|s| !s.is_empty())
                .collect();
            if !flags.contains(&"rust")
                && !flags.iter().all(|s| {
                    matches!(
                        *s,
                        "rust"
                            | "no_run"
                            | "should_panic"
                            | "compile_fail"
                            | "ignore"
                            | "edition2015"
                            | "edition2018"
                            | "edition2021"
                            | "edition2024"
                    ) || s.starts_with("ignore-")
                        || (s.len() == 5
                            && s.starts_with('E')
                            && s[1..].bytes().all(|b| b.is_ascii_digit()))
                })
            {
                foreign_fence = Some(mark.to_string().repeat(len));
                continue;
            }
            let edition = flags
                .iter()
                .find_map(|s| s.strip_prefix("edition"))
                .and_then(|s| match s {
                    "2015" => Some(flux_syntax::Edition::E2015),
                    "2018" => Some(flux_syntax::Edition::E2018),
                    "2021" => Some(flux_syntax::Edition::E2021),
                    "2024" => Some(flux_syntax::Edition::E2024),
                    _ => None,
                });
            active = Some((
                mark.to_string().repeat(len),
                Example {
                    line: line + 1,
                    ignore: flags.iter().any(|s| {
                        *s == "ignore"
                            || s.strip_prefix("ignore-").is_some_and(|s| {
                                std::env::consts::OS.contains(s)
                                    || std::env::consts::ARCH.contains(s)
                            })
                    }),
                    no_run: flags.contains(&"no_run"),
                    compile_fail: flags.contains(&"compile_fail"),
                    should_panic: flags.contains(&"should_panic"),
                    edition,
                    ..Example::default()
                },
            ));
        }
    }
    result
}

pub(crate) fn run(
    sources: &[SourceInput],
    root: usize,
    dependencies: &[(String, usize)],
    output: &Path,
    options: crate::BuildOptions,
    jobs: usize,
    native: &flux_codegen::NativeLink,
) -> Result<ExitCode, String> {
    let mut db = flux_hir::SourceDb::new();
    db.set_panic_strategy(options.panic_strategy);
    db.set_debug_assertions(options.debug_assertions());
    let mut graph = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let file = if let Some(macros) = &source.host_macros {
            db.add_proc_macro_crate(source.file.clone(), macros.clone())
        } else {
            db.load_with_edition(&source.file, source.edition)?
        };
        let mut cfg = source.cfg.clone();
        if index == root {
            cfg.names.push(flux_intern::Symbol::intern("doctest"));
        }
        db.set_crate_cfg(file, cfg)?;
        if let Some(env) = &source.environment {
            db.set_crate_env(file, env.clone())?;
        }
        graph.push(flux_hir::SourceCrate {
            root: file,
            name: flux_intern::Symbol::intern(&source.name),
            dependencies: source
                .dependencies
                .iter()
                .map(|(name, id)| (flux_intern::Symbol::intern(name), *id))
                .collect(),
        });
    }
    let hir = flux_hir::lower_sources(&mut db, &graph);
    let krate = hir
        .items
        .crates
        .iter()
        .enumerate()
        .find_map(|(index, data)| {
            (hir.items.modules[data.root].file == graph[root].root)
                .then_some(flux_hir::CrateId(index as u32))
        })
        .ok_or("missing documentation crate")?;
    let docs = flux_hir::documentation(&hir.items, krate)?;
    let directory = output
        .parent()
        .ok_or("doctest output has no parent")?
        .join("doctests");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    let mut index = 0;
    for doc in docs {
        for example in markdown(&doc.markdown) {
            index += 1;
            let label = format!(
                "{} - example {}",
                doc.path.display(),
                doc.line + example.line - 1
            );
            if example.ignore {
                ignored += 1;
                println!("test {label} ... ignored");
                continue;
            }
            let path = directory.join(format!("example-{index}.rs"));
            let binary = directory.join(format!("example-{index}"));
            let parse = flux_syntax::parse_source_file(&example.code);
            let has_main=flux_syntax::ast::SourceFile::cast(parse.tree().root()).is_some_and(|s|s.items().any(|i|matches!(i,flux_syntax::ast::Item::Fn(f) if f.name().is_some_and(|n|n.text()=="main"))));
            let code = if has_main {
                example.code
            } else {
                let root_node =
                    flux_syntax::ast::SourceFile::cast(parse.tree().root()).expect("source root");
                let mut body = example.code.clone();
                let mut header = String::new();
                let attrs: Vec<_> = root_node.attrs().collect();
                for attr in attrs.iter().rev() {
                    let range = attr.syntax().text_range();
                    let text = attr.syntax().text();
                    header = format!("{text}\n{header}");
                    body.replace_range(range.start() as usize..range.end() as usize, "");
                }
                if body.contains("Ok::<") {
                    format!(
                        "{header}fn main() {{let result=(||{{\n{body}\n}})();result.unwrap();}}\n"
                    )
                } else {
                    format!("{header}fn main() {{\n{body}\n}}\n")
                }
            };
            std::fs::write(&path, code).map_err(|e| e.to_string())?;
            let mut graph = sources.to_vec();
            let mut input = SourceInput::standalone(&path, "doctest");
            input.edition = example.edition.unwrap_or(sources[root].edition);
            input.cfg = sources[root].cfg.clone();
            input.environment = sources[root].environment.clone();
            input.compilation_dir = sources[root].compilation_dir.clone();
            input.dependencies = dependencies.to_vec();
            graph.push(input);
            let status = super::cargo::native::build::run(
                &graph,
                graph.len() - 1,
                &binary,
                options,
                jobs,
                false,
                native,
            );
            let compiled = matches!(&status,Ok(code) if *code==ExitCode::SUCCESS);
            let okay = if example.compile_fail {
                matches!(&status,Ok(code) if *code!=ExitCode::SUCCESS)
            } else if !compiled {
                false
            } else if example.no_run {
                true
            } else {
                let directory = sources[root]
                    .environment
                    .as_ref()
                    .and_then(|env| env.get("CARGO_MANIFEST_DIR"))
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        sources[root]
                            .file
                            .parent()
                            .unwrap_or(Path::new("."))
                            .to_owned()
                    });
                let status = Command::new(&binary)
                    .current_dir(directory)
                    .status()
                    .map_err(|e| e.to_string())?;
                if example.should_panic {
                    status.code() == Some(101)
                } else {
                    status.success()
                }
            };
            if let Err(error) = status {
                eprintln!("{error}");
            }
            if okay {
                passed += 1;
                println!("test {label} ... ok");
            } else {
                failed += 1;
                println!("test {label} ... FAILED");
            }
        }
    }
    println!("doctest result: {passed} passed; {failed} failed; {ignored} ignored");
    Ok(if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(101)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fences_hidden_lines_and_languages() {
        let cases = markdown(
            "```python\nnot rust\n```\n```rust,no_run\n# let x=1;\n##[allow(unused)]\nassert_eq!(x,1);\n```\n```compile_fail,E9999\nbad();\n```\n",
        );
        assert_eq!(cases.len(), 2);
        assert!(cases[0].no_run);
        assert!(cases[0].code.contains("let x=1;"));
        assert!(cases[0].code.contains("#[allow(unused)]"));
        assert!(cases[1].compile_fail);
    }
}
