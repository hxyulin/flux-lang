//! Builds isolated macro hosts with Flux's token API, using rustc as a bootstrap compiler.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use flux_hir::{ProcMacro, ProcMacroExpander, ProcMacroInput, ProcMacroKind, ProcMacroToken};
use flux_intern::Symbol;
use flux_proc_macro::{
    Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
};
use flux_syntax::Edition;
use flux_syntax::ast::{self, AstNode};
use flux_syntax::{SyntaxKind, T, TextRange, TokenOrigin};
use std::str::FromStr;
use std::sync::Arc;

/// Compiles a local macro crate and returns its exported macro descriptors.
pub fn build_host(
    source: &Path,
    crate_name: &str,
    edition: Edition,
    output: &Path,
    dependencies: &[(String, PathBuf)],
    search_paths: &[PathBuf],
    manifest_dir: &Path,
) -> Result<Vec<ProcMacro>, String> {
    std::fs::create_dir_all(output)
        .map_err(|e| format!("cannot create procedural macro host directory: {e}"))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let source = source.canonicalize().map_err(|e| e.to_string())?;
    let edition = match edition {
        Edition::E2015 => "2015",
        Edition::E2018 => "2018",
        Edition::E2021 => "2021",
        Edition::E2024 => "2024",
    };

    // Let rustc enforce proc-macro crate/export rules before transforming the crate
    // into a host executable. Ordinary executable compilation cannot enforce them.
    let mut validate = rustc(edition, dependencies, search_paths);
    validate
        .current_dir(manifest_dir)
        .env("CARGO_MANIFEST_DIR", manifest_dir);
    validate
        .arg(&source)
        .args([
            "--crate-name",
            crate_name,
            "--crate-type",
            "proc-macro",
            "--extern",
            "proc_macro",
            "--emit",
            "metadata",
            "-o",
        ])
        .arg(output.join("validated.rmeta"));
    run(&mut validate, "validate procedural macro crate")?;
    let prepared = prepare_source(&source, &output)?;
    let exports = prepared.exports;
    let transformed = prepared.text;
    let api = output.join("libflux_proc_macro.rlib");
    flux_proc_macro::write_sources(&output).map_err(|e| e.to_string())?;
    let runtime = output.join("lib.rs");
    let mut compile_api = Command::new("rustc");
    compile_api
        .arg(runtime)
        .args([
            "--edition=2024",
            "--crate-name",
            "proc_macro",
            "--crate-type",
            "rlib",
            "-o",
        ])
        .arg(&api);
    run(&mut compile_api, "build procedural macro token API")?;

    let mut arms = String::new();
    for export in &exports {
        let function = export.function.as_str();
        let invocation = if export.kind == ProcMacroKind::Attribute {
            format!("crate::{function}(attribute,input)")
        } else {
            format!("crate::{function}(input)")
        };
        arms.push_str(&format!("{:?} => {invocation},\n", export.name));
    }
    let runner = format!(
        r#"
{transformed}
extern crate proc_macro as __flux_proc_macro;
fn main() {{
    __flux_proc_macro::__flux_enter_host();
    let result = std::panic::catch_unwind(|| -> Result<(), String> {{
        let args: Vec<_> = std::env::args_os().collect();
        if args.len()!=5 {{return Err("expected macro name and protocol paths".into());}}
        let read = |path: &std::ffi::OsStr| -> Result<__flux_proc_macro::TokenStream,String> {{
            let bytes=std::fs::read(path).map_err(|e|e.to_string())?;
            __flux_proc_macro::TokenStream::from_wire(&bytes).map_err(|e|e.to_string())
        }};
        let attribute=read(&args[2])?;
        let input=read(&args[3])?;
        let _ = (&attribute, &input);
        let output: __flux_proc_macro::TokenStream=match args[1].to_str().ok_or("invalid macro name")? {{
            {arms}
            _=>return Err("unknown procedural macro".into()),
        }};
        std::fs::write(&args[4],output.to_wire()).map_err(|e|e.to_string())
    }});
    match result {{Ok(Ok(()))=>{{}},Ok(Err(e))=>{{eprintln!("{{e}}");std::process::exit(1);}},Err(_)=>std::process::exit(1)}}
}}
"#
    );
    let runner_path = prepared.path;
    std::fs::write(&runner_path, runner).map_err(|e| e.to_string())?;
    let executable = output.join("host");
    let mut compile = rustc(edition, dependencies, search_paths);
    compile
        .current_dir(manifest_dir)
        .env("CARGO_MANIFEST_DIR", manifest_dir);
    compile
        .arg(runner_path)
        .arg("--remap-path-prefix")
        .arg(format!("{}=/", prepared.mirror.display()))
        .args([
            "--cfg",
            "proc_macro",
            "-Aexplicit_builtin_cfgs_in_flags",
            "--crate-name",
            crate_name,
            "--extern",
        ])
        .arg(format!("proc_macro={}", api.display()))
        .arg("-o")
        .arg(&executable);
    run(&mut compile, "build procedural macro host")?;
    Ok(descriptors(&executable, exports))
}

fn rustc(edition: &str, dependencies: &[(String, PathBuf)], search_paths: &[PathBuf]) -> Command {
    let mut cmd = Command::new("rustc");
    cmd.arg(format!("--edition={edition}"));
    for (name, path) in dependencies {
        cmd.arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    for path in search_paths {
        cmd.arg("-L").arg(format!("dependency={}", path.display()));
    }
    cmd
}
fn run(command: &mut Command, action: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|e| format!("cannot {action}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cannot {action}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
pub struct Export {
    pub name: String,
    pub function: String,
    pub kind: ProcMacroKind,
    pub helpers: Vec<String>,
}

pub struct PreparedSource {
    pub text: String,
    /// Physical root bytes used to generate the wrapper.
    pub original: String,
    pub exports: Vec<Export>,
    pub path: PathBuf,
    pub mirror: PathBuf,
}

/// Prepares an output-owned source mirror so rustc retains source-relative loading.
pub fn prepare_source(source: &Path, output: &Path) -> Result<PreparedSource, String> {
    prepare_source_with_cfg(source, output, &host_cfg()?)
}

pub fn prepare_source_with_cfg(
    source: &Path,
    output: &Path,
    cfg: &HashSet<String>,
) -> Result<PreparedSource, String> {
    let text = std::fs::read_to_string(source)
        .map_err(|e| format!("cannot read procedural macro source: {e}"))?;
    let original_text = text;
    let (text, exports) = transform(&original_text, cfg)?;
    let mirror = output.join("sources");
    let parent = source
        .parent()
        .ok_or("expected procedural macro source directory")?;
    let path = mirror.join(
        source
            .strip_prefix(Path::new("/"))
            .map_err(|_| "expected absolute macro source path")?,
    );
    // Only the path to the generated root consists of real directories. Each
    // sibling is a link into the original tree. No source directory is changed.
    let mut original = PathBuf::from("/");
    let mut destination = mirror.clone();
    std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
    let components: Vec<_> = parent
        .strip_prefix("/")
        .map_err(|_| "expected absolute macro source path")?
        .components()
        .collect();
    for next in components.iter().map(Some).chain(std::iter::once(None)) {
        let next_name = next.map(|component| component.as_os_str());
        for entry in std::fs::read_dir(&original)
            .map_err(|e| format!("cannot mirror macro source directory: {e}"))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            if next_name == Some(entry.file_name().as_os_str())
                || (next.is_none() && entry.path() == source)
            {
                continue;
            }
            let link = destination.join(entry.file_name());
            if std::fs::symlink_metadata(&link).is_ok() {
                continue;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(entry.path(), &link)
                .map_err(|e| format!("cannot link macro source: {e}"))?;
            #[cfg(not(unix))]
            return Err(
                "procedural macro source mirrors are not supported on this host yet".into(),
            );
        }
        if let Some(component) = next {
            original.push(component.as_os_str());
            destination.push(component.as_os_str());
            std::fs::create_dir_all(&destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(PreparedSource {
        text,
        original: original_text,
        exports,
        path,
        mirror,
    })
}

fn host_cfg() -> Result<HashSet<String>, String> {
    let output = Command::new("rustc")
        .args(["--print", "cfg", "--crate-type", "proc-macro"])
        .output()
        .map_err(|e| format!("cannot read macro host configuration: {e}"))?;
    if !output.status.success() {
        return Err("cannot read macro host configuration".into());
    }
    let mut cfg: HashSet<_> = String::from_utf8(output.stdout)
        .map_err(|e| e.to_string())?
        .lines()
        .map(str::to_owned)
        .collect();
    cfg.insert("proc_macro".into());
    Ok(cfg)
}

fn predicate(tokens: &[TokenTree], cfg: &HashSet<String>) -> Result<bool, String> {
    let tokens = if matches!(tokens.last(), Some(TokenTree::Punct(p)) if p.as_char() == ',') {
        &tokens[..tokens.len() - 1]
    } else {
        tokens
    };
    match tokens {
        [TokenTree::Ident(name)] => Ok(cfg.contains(&name.to_string())),
        [
            TokenTree::Ident(name),
            TokenTree::Punct(equal),
            TokenTree::Literal(value),
        ] if equal.as_char() == '=' => {
            let value = value.to_string();
            let decoded = if let Some(raw) = value.strip_prefix('r') {
                let hashes = raw.bytes().take_while(|&byte| byte == b'#').count();
                raw.get(hashes + 1..raw.len().saturating_sub(hashes + 1))
                    .ok_or("invalid cfg string")?
                    .to_owned()
            } else {
                serde_json::from_str::<String>(&value)
                    .map_err(|_| "unsupported cfg string literal")?
            };
            Ok(cfg.contains(&format!(
                "{name}={}",
                serde_json::to_string(&decoded).map_err(|e| e.to_string())?
            )))
        }
        [TokenTree::Ident(name), TokenTree::Group(group)]
            if group.delimiter() == Delimiter::Parenthesis =>
        {
            let values = group.stream().into_iter().collect::<Vec<_>>();
            let mut arguments: Vec<_> = values
                .split(|token| matches!(token, TokenTree::Punct(p) if p.as_char() == ','))
                .collect();
            if arguments.last().is_some_and(|part| part.is_empty()) {
                arguments.pop();
            }
            if arguments.iter().any(|part| part.is_empty()) {
                return Err("expected a cfg predicate".into());
            }
            let values = arguments
                .iter()
                .map(|part| predicate(part, cfg))
                .collect::<Result<Vec<_>, _>>()?;
            match name.to_string().as_str() {
                "all" => Ok(values.into_iter().all(|value| value)),
                "any" => Ok(values.into_iter().any(|value| value)),
                "not" => match values.as_slice() {
                    [value] => Ok(!value),
                    _ => Err("expected one cfg predicate".into()),
                },
                _ => Err("unsupported cfg predicate".into()),
            }
        }
        _ => Err("unsupported cfg predicate".into()),
    }
}

fn attribute_tokens(attr: ast::Attr<'_>) -> Result<Vec<TokenTree>, String> {
    let stream: TokenStream = attr
        .syntax()
        .text()
        .parse()
        .map_err(|_| "invalid procedural macro attribute")?;
    let mut tokens = stream.into_iter();
    tokens.next();
    let Some(TokenTree::Group(group)) = tokens.next() else {
        return Err("expected attribute tokens".into());
    };
    Ok(group.stream().into_iter().collect())
}

fn expand_attribute(
    tokens: Vec<TokenTree>,
    cfg: &HashSet<String>,
    output: &mut Vec<Vec<TokenTree>>,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("recursion limit reached in conditional macro attributes".into());
    }
    if matches!(tokens.first(), Some(TokenTree::Ident(name)) if name == "cfg_attr") {
        let [_, TokenTree::Group(arguments)] = tokens.as_slice() else {
            return Err("expected cfg_attr arguments".into());
        };
        let arguments: Vec<_> = arguments.stream().into_iter().collect();
        let mut parts =
            arguments.split(|token| matches!(token, TokenTree::Punct(p) if p.as_char() == ','));
        if predicate(parts.next().ok_or("expected cfg_attr predicate")?, cfg)? {
            for part in parts.filter(|part| !part.is_empty()) {
                expand_attribute(part.to_vec(), cfg, output, depth + 1)?;
            }
        }
    } else {
        output.push(tokens);
    }
    Ok(())
}

pub fn item_enabled(
    node: flux_syntax::SyntaxNode<'_>,
    cfg: &HashSet<String>,
) -> Result<bool, String> {
    let mut attributes = Vec::new();
    for attr in node.children().filter_map(ast::Attr::cast) {
        expand_attribute(attribute_tokens(attr)?, cfg, &mut attributes, 0)?;
    }
    for tokens in attributes {
        if matches!(tokens.first(), Some(TokenTree::Ident(name)) if name == "cfg") {
            let [_, TokenTree::Group(group)] = tokens.as_slice() else {
                return Err("expected cfg predicate".into());
            };
            if !predicate(&group.stream().into_iter().collect::<Vec<_>>(), cfg)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn transform(text: &str, cfg: &HashSet<String>) -> Result<(String, Vec<Export>), String> {
    let parse = flux_syntax::parse_source_file(text);
    if !parse.errors().is_empty() {
        return Err("procedural macro source uses syntax not supported yet".into());
    }
    let tree = parse.tree();
    let root = tree.root();
    let mut edits = Vec::new();
    let mut exports = Vec::new();
    for node in root.children() {
        let Some(function) = ast::Fn::cast(node) else {
            continue;
        };
        let mut attributes = Vec::new();
        for attr in function.attrs() {
            let mut expanded = Vec::new();
            expand_attribute(attribute_tokens(attr)?, cfg, &mut expanded, 0)?;
            attributes.push((attr, expanded));
        }
        let enabled = attributes
            .iter()
            .flat_map(|(_, expanded)| expanded)
            .try_fold(true, |enabled, tokens| -> Result<bool, String> {
                if matches!(tokens.first(), Some(TokenTree::Ident(name)) if name == "cfg") {
                    let [_, TokenTree::Group(group)] = tokens.as_slice() else {
                        return Err("expected cfg predicate".into());
                    };
                    Ok(enabled && predicate(&group.stream().into_iter().collect::<Vec<_>>(), cfg)?)
                } else {
                    Ok(enabled)
                }
            })?;
        if !enabled {
            continue;
        }
        if function.name().is_some_and(|name| name.text() == "main") {
            return Err("a main function in procedural macro crates is not supported yet".into());
        }
        for (attr, expanded) in attributes {
            let mut retained = Vec::new();
            for tokens in expanded {
                let Some(TokenTree::Ident(name)) = tokens.first() else {
                    retained.push(tokens);
                    continue;
                };
                let kind = match name.to_string().as_str() {
                    "proc_macro" => ProcMacroKind::Function,
                    "proc_macro_attribute" => ProcMacroKind::Attribute,
                    "proc_macro_derive" => ProcMacroKind::Derive,
                    _ => {
                        retained.push(tokens);
                        continue;
                    }
                };
                if kind != ProcMacroKind::Derive && tokens.len() != 1 {
                    return Err("procedural macro entry attributes do not accept arguments".into());
                }
                let function = function
                    .name()
                    .ok_or("expected a procedural macro function name")?
                    .text()
                    .to_owned();
                let (name, helpers) = if kind == ProcMacroKind::Derive {
                    if tokens.len() != 2 {
                        return Err("expected derive arguments".into());
                    }
                    let Some(TokenTree::Group(args)) = tokens.get(1) else {
                        return Err("expected derive arguments".into());
                    };
                    if args.delimiter() != Delimiter::Parenthesis {
                        return Err("expected parenthesized derive arguments".into());
                    }
                    let mut args = args.stream().into_iter();
                    let Some(TokenTree::Ident(name)) = args.next() else {
                        return Err("expected derive name".into());
                    };
                    if flux_syntax::LexedStr::with_edition(&name.to_string(), Edition::E2024)
                        .kind(0)
                        != flux_syntax::SyntaxKind::IDENT
                    {
                        return Err("expected a derive identifier".into());
                    }
                    let mut helpers = Vec::new();
                    let comma = |token: &TokenTree| matches!(token, TokenTree::Punct(p) if p.as_char() == ',');
                    let rest: Vec<_> = args.collect();
                    if !rest.is_empty() {
                        if !comma(&rest[0]) {
                            return Err("expected `,` after derive name".into());
                        }
                        if rest.len() > 1 {
                            let helper = match &rest[1..] {
                                [TokenTree::Ident(name), TokenTree::Group(group)]
                                    if name == "attributes" =>
                                {
                                    group
                                }
                                [TokenTree::Ident(name), TokenTree::Group(group), trailing]
                                    if name == "attributes" && comma(trailing) =>
                                {
                                    group
                                }
                                _ => return Err("expected derive helper attributes".into()),
                            };
                            if helper.delimiter() != Delimiter::Parenthesis {
                                return Err("expected parenthesized helper attributes".into());
                            }
                            let mut want_name = true;
                            for token in helper.stream() {
                                match token {
                                    TokenTree::Ident(ident) if want_name => {
                                        if flux_syntax::LexedStr::with_edition(
                                            &ident.to_string(),
                                            Edition::E2024,
                                        )
                                        .kind(0)
                                            != flux_syntax::SyntaxKind::IDENT
                                        {
                                            return Err("expected a helper identifier".into());
                                        }
                                        helpers.push(ident.to_string());
                                        want_name = false;
                                    }
                                    token if !want_name && comma(&token) => want_name = true,
                                    _ => return Err("expected comma-separated helper names".into()),
                                }
                            }
                        }
                    }
                    (name.to_string(), helpers)
                } else {
                    (function.clone(), Vec::new())
                };
                exports.push(Export {
                    name,
                    function,
                    kind,
                    helpers,
                });
            }
            let mut replacement = retained
                .into_iter()
                .map(|tokens| format!("#[{}]", tokens.into_iter().collect::<TokenStream>()))
                .collect::<String>();
            replacement.push_str(
                &"\n".repeat(
                    attr.syntax()
                        .text()
                        .bytes()
                        .filter(|&byte| byte == b'\n')
                        .count(),
                ),
            );
            let range = attr.syntax().text_range();
            edits.push((range.start() as usize, range.end() as usize, replacement));
        }
    }
    edits.sort_by_key(|(start, _, _)| *start);
    let mut transformed = text.to_owned();
    for (start, end, replacement) in edits.into_iter().rev() {
        transformed.replace_range(start..end, &replacement);
    }
    Ok((transformed, exports))
}

/// Registers a local executable through HIR's shared host-expander interface.
pub fn descriptors(executable: &Path, exports: Vec<Export>) -> Vec<ProcMacro> {
    let identity = super::flib::HostIdentity::local(executable).ok();
    let executable = identity.as_ref().map_or_else(
        || executable.to_owned(),
        |identity| identity.local_path().to_owned(),
    );
    exports
        .into_iter()
        .map(|export| ProcMacro {
            name: Symbol::intern(&export.name),
            kind: export.kind,
            helpers: Some(export.helpers.iter().map(|s| Symbol::intern(s)).collect()),
            expander: Arc::new(LocalHost {
                identity: identity.clone(),
                executable: executable.clone(),
                name: export.name,
            }),
        })
        .collect()
}

#[derive(Debug)]
struct LocalHost {
    executable: PathBuf,
    name: String,
    identity: Option<super::flib::HostIdentity>,
}

pub(super) fn restore_host(
    identity: super::flib::HostIdentity,
    name: String,
) -> Arc<dyn ProcMacroExpander> {
    Arc::new(LocalHost {
        executable: identity.local_path().to_owned(),
        name,
        identity: Some(identity),
    })
}
impl ProcMacroExpander for LocalHost {
    fn flib_descriptor(&self) -> Option<Vec<u8>> {
        self.identity.as_ref()?.descriptor(&self.name).ok()
    }
    fn expand(&self, invocation: ProcMacroInput) -> Result<Vec<ProcMacroToken>, String> {
        if let Some(identity) = &self.identity {
            identity.verify()?;
        }
        if invocation
            .tokens
            .iter()
            .chain(invocation.attributes.iter().flatten())
            .any(|token| token.origin.ctxt != 0)
        {
            return Err(
                "proc-macro inputs with non-root macro_rules hygiene are not supported yet".into(),
            );
        }
        let body = input_tokens(&invocation.tokens)?;
        let attributes = input_tokens(invocation.attributes.as_deref().unwrap_or_default())?;
        let output = execute(
            &self.executable,
            &self.name,
            attributes,
            body,
            &invocation,
            std::time::Duration::from_secs(60),
        )?;
        if let Some(identity) = &self.identity {
            identity.verify()?;
        }
        output_tokens(output, &invocation)
    }
}

fn execute(
    executable: &Path,
    name: &str,
    attributes: TokenStream,
    input: TokenStream,
    invocation: &ProcMacroInput,
    timeout: std::time::Duration,
) -> Result<TokenStream, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Files(PathBuf);
    impl Drop for Files {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = std::env::temp_dir().join(format!(
        "flux-local-proc-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir)
        .map_err(|e| format!("cannot create procedural macro request: {e}"))?;
    let files = Files(dir);
    let attr = files.0.join("attribute");
    let source = files.0.join("input");
    let output = files.0.join("output");
    std::fs::write(&attr, attributes.to_wire())
        .and_then(|()| std::fs::write(&source, input.to_wire()))
        .map_err(|e| format!("cannot write procedural macro request: {e}"))?;
    let mut child = Command::new(executable)
        .arg(name)
        .arg(&attr)
        .arg(&source)
        .arg(&output)
        .current_dir(&invocation.current_dir)
        .envs(&invocation.environment)
        .env("FLUX_PROC_MACRO_NAME", name)
        .env("FLUX_PROC_MACRO_ATTRIBUTE", &attr)
        .env("FLUX_PROC_MACRO_INPUT", &source)
        .env("FLUX_PROC_MACRO_OUTPUT", &output)
        .spawn()
        .map_err(|e| format!("cannot run procedural macro: {e}"))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(match result {
                    Err(error) => format!("cannot wait for procedural macro: {error}"),
                    _ => "procedural macro host timed out".into(),
                });
            }
        }
    };
    if !status.success() {
        return Err(format!("procedural macro `{name}` failed ({status})"));
    }
    if std::fs::metadata(&output)
        .map_err(|e| format!("cannot read procedural macro output: {e}"))?
        .len()
        > 64 * 1024 * 1024
    {
        return Err("procedural macro output exceeds protocol limit".into());
    }
    let bytes =
        std::fs::read(output).map_err(|e| format!("cannot read procedural macro output: {e}"))?;
    TokenStream::from_wire(&bytes).map_err(|e| e.to_string())
}

fn token_span(token: &ProcMacroToken) -> Span {
    Span::new(token.origin.range.start(), token.origin.range.end())
}
fn identifier(text: &str, span: Span) -> Result<Ident, String> {
    let raw = text.strip_prefix("r#");
    let name = raw.unwrap_or(text);
    if !flux_proc_macro::is_identifier(name)
        || (raw.is_some() && matches!(name, "self" | "super" | "crate" | "Self"))
    {
        return Err("invalid identifier in procedural macro input".into());
    }
    Ok(if raw.is_some() {
        Ident::new_raw(name, span)
    } else {
        Ident::new(name, span)
    })
}
fn is_open(kind: SyntaxKind) -> bool {
    matches!(kind, T!['('] | T!['['] | T!['{'])
}
fn is_close(kind: SyntaxKind) -> bool {
    matches!(kind, T![')'] | T![']'] | T!['}'])
}
fn input_tokens(tokens: &[ProcMacroToken]) -> Result<TokenStream, String> {
    fn read(
        tokens: &[ProcMacroToken],
        cursor: &mut usize,
        close: Option<SyntaxKind>,
        depth: usize,
    ) -> Result<TokenStream, String> {
        if depth > 128 {
            return Err("proc-macro token nesting limit exceeded".into());
        }
        let mut output = Vec::new();
        while let Some(token) = tokens.get(*cursor) {
            if Some(token.kind) == close {
                *cursor += 1;
                return Ok(output.into_iter().collect());
            }
            *cursor += 1;
            let group = match token.kind {
                T!['('] => Some((Delimiter::Parenthesis, T![')'])),
                T!['['] => Some((Delimiter::Bracket, T![']'])),
                T!['{'] => Some((Delimiter::Brace, T!['}'])),
                _ => None,
            };
            if let Some((delimiter, close)) = group {
                let stream = read(tokens, cursor, Some(close), depth + 1)?;
                let delimiter = if token.origin.range.is_empty()
                    && token.origin.group != TokenOrigin::SYNTHETIC
                    && delimiter == Delimiter::Parenthesis
                {
                    Delimiter::None
                } else {
                    delimiter
                };
                let mut group = Group::new(delimiter, stream);
                group.set_span(Span::new(
                    token.origin.range.start(),
                    tokens[cursor.saturating_sub(1)].origin.range.end(),
                ));
                output.push(TokenTree::Group(group));
            } else if is_close(token.kind) {
                return Err("unexpected closing delimiter in procedural macro input".into());
            } else if token.kind == SyntaxKind::LIFETIME_IDENT {
                let mut quote = Punct::new('\'', Spacing::Joint);
                quote.set_span(token_span(token));
                output.push(TokenTree::Punct(quote));
                output.push(TokenTree::Ident(identifier(
                    token.text.trim_start_matches('\''),
                    token_span(token),
                )?));
            } else if token.kind.is_literal() {
                let mut text = token.text.clone();
                let mut span = token_span(token);
                if !matches!(
                    token.kind,
                    SyntaxKind::INT_NUMBER | SyntaxKind::FLOAT_NUMBER
                ) && token.joint
                    && let Some(next) = tokens.get(*cursor)
                    && (next.kind == SyntaxKind::IDENT || next.kind.is_keyword())
                    && next.origin.group == token.origin.group
                    && next.origin.ctxt == token.origin.ctxt
                    && (token.origin.range.end() == next.origin.range.start()
                        || token.origin.range == next.origin.range)
                {
                    if next.text.starts_with("r#") {
                        return Err(
                            "raw-identifier literal suffix boundaries are not supported yet".into(),
                        );
                    }
                    text.push_str(&next.text);
                    span = Span::new(token.origin.range.start(), next.origin.range.end());
                    *cursor += 1;
                }
                let mut literal = Literal::from_str(&text).map_err(|e| e.to_string())?;
                literal.set_span(span);
                output.push(TokenTree::Literal(literal));
            } else if token.kind.is_punct() && token.kind != T![_] {
                let mut chars = token.text.chars().peekable();
                while let Some(ch) = chars.next() {
                    let joint = chars.peek().is_some()
                        || token.joint
                            && tokens.get(*cursor).is_some_and(|next| {
                                next.kind.is_punct() && !is_open(next.kind) && !is_close(next.kind)
                            });
                    let mut punct = Punct::new(
                        ch,
                        if joint {
                            Spacing::Joint
                        } else {
                            Spacing::Alone
                        },
                    );
                    punct.set_span(token_span(token));
                    output.push(TokenTree::Punct(punct));
                }
            } else {
                output.push(TokenTree::Ident(identifier(
                    &token.text,
                    token_span(token),
                )?));
            }
        }
        if close.is_some() {
            Err("unclosed delimiter in procedural macro input".into())
        } else {
            Ok(output.into_iter().collect())
        }
    }
    read(tokens, &mut 0, None, 0)
}

fn output_tokens(
    output: TokenStream,
    invocation: &ProcMacroInput,
) -> Result<Vec<ProcMacroToken>, String> {
    fn push(
        out: &mut Vec<ProcMacroToken>,
        text: &str,
        span: Span,
        joint: bool,
        invocation: &ProcMacroInput,
    ) -> Result<(), String> {
        if out.len() > 1_000_000 {
            return Err("host token output limit exceeded".into());
        }
        let origin = TokenOrigin {
            range: if span.byte_end() != 0 {
                TextRange::new(span.byte_start(), span.byte_end())
            } else {
                invocation.call
            },
            group: TokenOrigin::SYNTHETIC,
            ctxt: 0,
        };
        if text == "'" {
            out.push(ProcMacroToken {
                edition: invocation.source.edition,
                kind: SyntaxKind::ERROR,
                text: text.into(),
                joint,
                origin,
            });
            return Ok(());
        }
        let lexed = flux_syntax::LexedStr::with_edition(text, invocation.source.edition);
        if !lexed.errors().is_empty() || (0..lexed.len()).any(|i| lexed.kind(i).is_trivia()) {
            return Err("invalid token from local procedural macro".into());
        }
        let significant: Vec<_> = (0..lexed.len())
            .filter(|&i| !lexed.kind(i).is_trivia())
            .collect();
        for (position, &index) in significant.iter().enumerate() {
            out.push(ProcMacroToken {
                edition: invocation.source.edition,
                kind: lexed.kind(index),
                text: lexed.token_text(index).into(),
                joint: if position + 1 == significant.len() {
                    joint
                } else {
                    lexed.kind(index) != SyntaxKind::MINUS
                },
                origin,
            });
        }
        Ok(())
    }
    fn flatten(
        stream: TokenStream,
        out: &mut Vec<ProcMacroToken>,
        invocation: &ProcMacroInput,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 128 {
            return Err("host token output nesting limit exceeded".into());
        }
        for token in stream {
            match token {
                TokenTree::Group(group) => {
                    let (open, close) = match group.delimiter() {
                        Delimiter::Parenthesis | Delimiter::None => ("(", ")"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::Bracket => ("[", "]"),
                    };
                    let span = group.span();
                    push(
                        out,
                        open,
                        if span.byte_end() == 0 {
                            span
                        } else {
                            group.span_open()
                        },
                        false,
                        invocation,
                    )?;
                    if group.delimiter() == Delimiter::None
                        && let Some(token) = out.last_mut()
                    {
                        token.origin.range = TextRange::empty(invocation.call.start());
                        token.origin.group = 1;
                    }
                    flatten(group.stream(), out, invocation, depth + 1)?;
                    push(
                        out,
                        close,
                        if span.byte_end() == 0 {
                            span
                        } else {
                            group.span_close()
                        },
                        false,
                        invocation,
                    )?;
                    if group.delimiter() == Delimiter::None
                        && let Some(token) = out.last_mut()
                    {
                        token.origin.range = TextRange::empty(invocation.call.end());
                        token.origin.group = 1;
                    }
                }
                TokenTree::Ident(token) => {
                    push(out, &token.to_string(), token.span(), false, invocation)?
                }
                TokenTree::Literal(token) => {
                    push(out, &token.to_string(), token.span(), false, invocation)?
                }
                TokenTree::Punct(token) => push(
                    out,
                    &token.as_char().to_string(),
                    token.span(),
                    token.spacing() == Spacing::Joint,
                    invocation,
                )?,
            }
        }
        Ok(())
    }
    let mut flat = Vec::new();
    flatten(output, &mut flat, invocation, 0)?;
    let mut joined: Vec<ProcMacroToken> = Vec::new();
    for token in flat {
        if let Some(previous) = joined.last_mut()
            && previous.text == "'"
            && previous.joint
            && (matches!(token.kind, SyntaxKind::IDENT | SyntaxKind::UNDERSCORE)
                || token.kind.is_keyword())
        {
            previous.text.push_str(&token.text);
            previous.kind = SyntaxKind::LIFETIME_IDENT;
            previous.joint = token.joint;
        } else {
            joined.push(token);
        }
    }
    if joined.iter().any(|t| t.kind == SyntaxKind::ERROR) {
        return Err("unpaired lifetime punctuation from local macro".into());
    }
    let mut origins = std::collections::HashMap::new();
    for token in invocation
        .tokens
        .iter()
        .chain(invocation.attributes.iter().flatten())
    {
        origins
            .entry((token.origin.range, token.text.as_str()))
            .or_insert(token);
    }
    for token in &mut joined {
        if let Some(original) = origins.get(&(token.origin.range, token.text.as_str())) {
            token.origin = original.origin;
            token.edition = original.edition;
        }
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "flux-local-adapter-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn input(&self, text: &str) -> ProcMacroInput {
            let mut db = flux_hir::SourceDb::new();
            let root = db.add(self.0.join("input.rs"), text);
            let lexed = flux_syntax::LexedStr::new(text);
            let indices: Vec<_> = (0..lexed.len())
                .filter(|&index| !lexed.kind(index).is_trivia())
                .collect();
            let tokens = indices
                .iter()
                .enumerate()
                .map(|(position, &index)| ProcMacroToken {
                    edition: Edition::E2024,
                    kind: lexed.kind(index),
                    text: lexed.token_text(index).into(),
                    joint: indices.get(position + 1).is_some_and(|&next| {
                        lexed.token_range(index).end() == lexed.token_range(next).start()
                    }),
                    origin: TokenOrigin {
                        range: lexed.token_range(index),
                        group: 17,
                        ctxt: 0,
                    },
                })
                .collect();
            ProcMacroInput {
                invocation_crate: 1,
                tokens,
                attributes: None,
                source: db.shared(root),
                call: TextRange::new(0, text.len() as u32),
                range_base: 0,
                mixed_context: 42,
                environment: BTreeMap::new(),
                current_dir: self.0.clone(),
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn input_roundtrip_preserves_source_provenance_and_lifetimes() {
        let fixture = Fixture::new();
        let invocation = fixture.input("r#type<'life> (value + 12)");
        let output = output_tokens(input_tokens(&invocation.tokens).unwrap(), &invocation).unwrap();
        assert_eq!(output.len(), invocation.tokens.len());
        for (actual, original) in output.iter().zip(&invocation.tokens) {
            assert_eq!(actual.text, original.text);
            assert_eq!(actual.kind, original.kind);
            assert_eq!(actual.origin, original.origin);
            assert_eq!(actual.edition, original.edition);
        }
        let suffix = fixture.input("\"word\"custom");
        let output = output_tokens(input_tokens(&suffix.tokens).unwrap(), &suffix).unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].text, "\"word\"");
        assert!(output[0].joint);
        assert_eq!(output[1].text, "custom");
    }

    #[test]
    fn local_host_uses_invocation_environment_and_directory() {
        let fixture = Fixture::new();
        let source = fixture.0.join("macro.rs");
        std::fs::write(&source, r#"
use proc_macro::TokenStream;
#[proc_macro] pub fn context(_:TokenStream)->TokenStream {
    format!("{:?}", format!("{}|{}", std::env::var("LOCAL_ENV_TEST").unwrap_or_else(|_|"absent".into()), std::env::current_dir().unwrap().display())).parse().unwrap()
}
"#).unwrap();
        let exports = build_host(
            &source,
            "context",
            Edition::E2024,
            &fixture.0.join("host"),
            &[],
            &[],
            &fixture.0,
        )
        .unwrap();
        let expander = &exports[0].expander;
        let before = std::env::var_os("LOCAL_ENV_TEST");
        let mut invocation = fixture.input("");
        invocation
            .environment
            .insert("LOCAL_ENV_TEST".into(), "caller".into());
        let output = expander.expand(invocation).unwrap();
        assert_eq!(
            output[0].text,
            format!("{:?}", format!("caller|{}", fixture.0.display()))
        );
        let output = expander.expand(fixture.input("")).unwrap();
        let inherited = before
            .clone()
            .and_then(|value| value.into_string().ok())
            .unwrap_or_else(|| "absent".into());
        assert_eq!(
            output[0].text,
            format!("{:?}", format!("{inherited}|{}", fixture.0.display()))
        );
        assert_eq!(std::env::var_os("LOCAL_ENV_TEST"), before);
    }

    #[test]
    #[cfg(unix)]
    fn timed_out_host_is_killed_and_reaped() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        let script = fixture.0.join("host");
        std::fs::write(
            &script,
            "#!/bin/sh\necho $$ > \"$PID_FILE\"\nexec /bin/sleep 60\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let pid_file = fixture.0.join("pid");
        let mut invocation = fixture.input("");
        invocation
            .environment
            .insert("PID_FILE".into(), pid_file.to_str().unwrap().into());
        let error = execute(
            &script,
            "hang",
            TokenStream::new(),
            TokenStream::new(),
            &invocation,
            std::time::Duration::from_millis(250),
        )
        .unwrap_err();
        assert!(error.contains("timed out"), "{error}");
        let pid = std::fs::read_to_string(pid_file).unwrap();
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .output()
                .unwrap()
                .status
                .success(),
            "timed-out host still exists"
        );
    }
}
