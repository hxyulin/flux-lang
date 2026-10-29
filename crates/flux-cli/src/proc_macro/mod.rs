//! Toolchain-matched, out-of-process host execution. No dylib enters Flux.
mod flib;
pub mod local;
pub use flib::FlibHostResolver;
mod protocol;
mod tokens;
use flux_hir::{ProcMacro, ProcMacroExpander, ProcMacroInput, ProcMacroToken};
use flux_intern::Symbol;
use protocol::{Message, Request, Response, ServerConfig, SpanMode};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

// The bidirectional format is explicitly a prototype, not a stable Rust API.
// Accept a reviewed schema only; do not send mismatched bytes to a host dylib.
const SUPPORTED_COMMIT: &str = "58cf8f9c35b355199727627d07ae34a4e2846d67";
const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_FRAME: usize = 32 * 1024 * 1024;

#[derive(Clone)]
struct Host {
    catalog: Arc<Catalog>,
    dylib: PathBuf,
    name: String,
    identity: flib::HostIdentity,
}
impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostProcMacro")
            .field("dylib", &self.dylib)
            .field("name", &self.name)
            .finish()
    }
}
struct Catalog {
    server: PathBuf,
    processes: std::sync::Mutex<CatalogState>,
}
struct CatalogState {
    idle: Option<Arc<Service>>,
    crates: std::collections::BTreeMap<u32, Arc<Service>>,
}
impl Catalog {
    fn for_crate(&self, id: u32) -> Result<Arc<Service>, String> {
        let mut state = self.processes.lock().expect("proc-macro catalog poisoned");
        if let Some(service) = state.crates.get(&id) {
            return Ok(service.clone());
        }
        let service = state
            .idle
            .take()
            .unwrap_or_else(|| Service::start(self.server.clone(), TIMEOUT));
        state.crates.insert(id, service.clone());
        Ok(service)
    }
}
/// Loads the host export catalog for one source graph using the matching toolchain.
/// Each invocation crate receives a separate persistent host process.
pub fn exports(
    rustc: &str,
    dylib: &Path,
    source: &Path,
    cfg: flux_hir::CrateCfg,
) -> Result<Vec<ProcMacro>, String> {
    if !rustc.contains(SUPPORTED_COMMIT) {
        return Err("proc-macro server protocol is not supported for this rustc commit".into());
    }
    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--print", "sysroot"])
        .output()
        .map_err(|e| format!("cannot find proc-macro server: {e}"))?;
    if !output.status.success() {
        return Err("rustc sysroot query failed".into());
    }
    let sysroot = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let server = Path::new(sysroot.trim()).join("libexec/rust-analyzer-proc-macro-srv");
    let identity = flib::HostIdentity::rustc(&server, dylib)?;
    let (server, dylib) = identity.rustc_paths();
    let server = server.to_owned();
    let dylib = dylib.to_owned();
    let service = Service::start(server.clone(), TIMEOUT);
    let names = match service.submit(Task::List(dylib.clone()))? {
        Reply::List(names) => names,
        _ => return Err("unexpected proc-macro export list".into()),
    };
    identity.verify()?;
    let catalog = Arc::new(Catalog {
        server,
        processes: std::sync::Mutex::new(CatalogState {
            idle: Some(service),
            crates: Default::default(),
        }),
    });
    let mut db = flux_hir::SourceDb::new();
    let root = db.load(source)?;
    db.set_crate_cfg(root, cfg)?;
    let helpers = flux_hir::proc_macro_declarations(db.file(root))?;
    names
        .into_iter()
        .map(|(name, kind)| {
            let kind = match kind {
                protocol::ProcMacroKind::CustomDerive => flux_hir::ProcMacroKind::Derive,
                protocol::ProcMacroKind::Attr => flux_hir::ProcMacroKind::Attribute,
                protocol::ProcMacroKind::Bang => flux_hir::ProcMacroKind::Function,
            };
            Ok(ProcMacro {
                name: Symbol::intern(&name),
                kind,
                // The compiled catalog also includes macro-generated exports.
                // Only derives need helper metadata; missing source metadata
                // must not turn unknown attributes into inert helpers.
                helpers: match kind {
                    flux_hir::ProcMacroKind::Derive => helpers
                        .get(&name)
                        .map(|names| names.iter().map(|s| Symbol::intern(s)).collect()),
                    _ => Some(Vec::new()),
                },
                expander: Arc::new(Host {
                    catalog: catalog.clone(),
                    dylib: dylib.to_owned(),
                    name,
                    identity: identity.clone(),
                }),
            })
        })
        .collect()
}

impl ProcMacroExpander for Host {
    fn flib_descriptor(&self) -> Option<Vec<u8>> {
        self.identity.descriptor(&self.name).ok()
    }
    fn expand(&self, input: ProcMacroInput) -> Result<Vec<ProcMacroToken>, String> {
        self.identity.verify()?;
        let service = self.catalog.for_crate(input.invocation_crate)?;
        let reply = service.submit(Task::Expand {
            dylib: self.dylib.clone(),
            name: self.name.clone(),
            input: Box::new(input),
        })?;
        self.identity.verify()?;
        match reply {
            Reply::Tokens(tokens) => Ok(tokens),
            _ => Err("unexpected proc-macro expansion reply".into()),
        }
    }
}

#[derive(Default)]
struct Watchdog {
    child: Option<Child>,
    finished: bool,
}
type ChildSlot = Arc<std::sync::Mutex<Watchdog>>;
enum Task {
    List(PathBuf),
    Expand {
        dylib: PathBuf,
        name: String,
        input: Box<ProcMacroInput>,
    },
}
enum Reply {
    List(Vec<(String, protocol::ProcMacroKind)>),
    Tokens(Vec<ProcMacroToken>),
}
struct Job {
    task: Task,
    reply: mpsc::SyncSender<Result<Reply, String>>,
}
struct Service {
    jobs: mpsc::Sender<Job>,
    child: ChildSlot,
    timeout: Duration,
}
impl Service {
    fn start(server: PathBuf, timeout: Duration) -> Arc<Self> {
        let child = Arc::new(std::sync::Mutex::new(Watchdog::default()));
        let watched = child.clone();
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let session: Result<Session, String> = (|| {
                let mut session = Session::start(&server, &watched)?;
                session.version()?;
                match session.request(
                    Request::SetConfig(ServerConfig {
                        span_mode: SpanMode::RustAnalyzer,
                    }),
                    None,
                )? {
                    Response::SetConfig(_) => Ok(session),
                    _ => Err("unexpected proc-macro configuration response".into()),
                }
            })();
            match session {
                Err(error) => {
                    stop_child(&watched);
                    for job in rx {
                        let _ = job.reply.send(Err(error.clone()));
                    }
                }
                Ok(mut session) => {
                    for job in rx {
                        let stopped = watched
                            .lock()
                            .expect("proc-macro watchdog poisoned")
                            .finished;
                        let result = if stopped {
                            Err("proc-macro host is no longer available".into())
                        } else {
                            execute(&mut session, job.task)
                        };
                        let _ = job.reply.send(result);
                    }
                }
            }
        });
        Arc::new(Service {
            jobs: tx,
            child,
            timeout,
        })
    }
    fn submit(&self, task: Task) -> Result<Reply, String> {
        if self
            .child
            .lock()
            .expect("proc-macro watchdog poisoned")
            .finished
        {
            return Err("proc-macro host is no longer available".into());
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.jobs
            .send(Job { task, reply: tx })
            .map_err(|_| "proc-macro worker terminated")?;
        match rx.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(_) => {
                stop_child(&self.child);
                Err("proc-macro host timed out or terminated".into())
            }
        }
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        stop_child(&self.child);
    }
}
fn stop_child(child: &ChildSlot) {
    let process = {
        let mut watched = child.lock().expect("proc-macro watchdog poisoned");
        watched.finished = true;
        watched.child.take()
    };
    if let Some(mut process) = process {
        let _ = process.kill();
        let _ = process.wait();
    }
}
fn execute(session: &mut Session, task: Task) -> Result<Reply, String> {
    match task {
        Task::List(dylib_path) => match session.request(
            Request::ListMacros(protocol::ListMacros { dylib_path }),
            None,
        )? {
            Response::ListMacros(result) => result.map(Reply::List),
            _ => Err("unexpected proc-macro list response".into()),
        },
        Task::Expand { dylib, name, input } => {
            let mut codec = tokens::Codec::new(&input)?;
            let body = codec.encode(&input.tokens)?;
            let attributes = input
                .attributes
                .as_ref()
                .map(|t| codec.encode(t))
                .transpose()?;
            let request = protocol::ExpandMacro {
                lib: dylib,
                env: input
                    .environment
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                current_dir: Some(
                    input
                        .current_dir
                        .to_str()
                        .ok_or("non-Unicode proc-macro working directory")?
                        .to_owned(),
                ),
                data: protocol::ExpandMacroData {
                    macro_body: body,
                    macro_name: name,
                    attributes,
                    has_global_spans: protocol::ExpnGlobals {
                        def_site: 2,
                        call_site: 0,
                        mixed_site: 1,
                    },
                    span_data_table: codec.spans.clone(),
                },
            };
            match session.request(Request::ExpandMacro(Box::new(request)), Some(&codec))? {
                Response::ExpandMacro(Ok(output)) => codec
                    .decode(output.tree, &output.span_data_table)
                    .map(Reply::Tokens),
                Response::ExpandMacro(Err(error)) => {
                    Err(format!("expansion panicked: {}", error.0))
                }
                _ => Err("unexpected proc-macro expansion response".into()),
            }
        }
    }
}
struct Session {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    watchdog: ChildSlot,
}
impl Session {
    fn start(server: &Path, watchdog: &ChildSlot) -> Result<Self, String> {
        let mut child = Command::new(server)
            .args(["--format", "bidirectional-postcard-prototype"])
            .env(
                "RUST_ANALYZER_INTERNALS_DO_NOT_USE",
                "flux toolchain host bridge",
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("cannot start toolchain proc-macro server: {e}"))?;
        let stdin = child.stdin.take().ok_or("missing proc-macro stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("missing proc-macro stdout")?);
        let mut watched = watchdog.lock().expect("proc-macro watchdog poisoned");
        if watched.finished {
            let _ = child.kill();
            let _ = child.wait();
            return Err("proc-macro host operation was cancelled".into());
        }
        watched.child = Some(child);
        Ok(Session {
            stdin,
            stdout,
            watchdog: watchdog.clone(),
        })
    }
    fn send(&mut self, msg: Message) -> Result<(), String> {
        let bytes = postcard::to_allocvec_cobs(&msg)
            .map_err(|e| format!("cannot encode proc-macro request: {e}"))?;
        self.stdin
            .write_all(&bytes)
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("proc-macro request failed: {e}"))
    }
    fn version(&mut self) -> Result<(), String> {
        match self.request(Request::ApiVersionCheck(protocol::ApiVersionCheck {}), None)? {
            Response::ApiVersionCheck(6) => Ok(()),
            _ => Err("unsupported proc-macro server protocol version".into()),
        }
    }
    fn request(
        &mut self,
        req: Request,
        codec: Option<&tokens::Codec<'_>>,
    ) -> Result<Response, String> {
        let result = self.roundtrip(req, codec);
        if result.is_err() {
            stop_child(&self.watchdog);
        }
        result
    }
    fn roundtrip(
        &mut self,
        req: Request,
        codec: Option<&tokens::Codec<'_>>,
    ) -> Result<Response, String> {
        self.send(Message::Request(req))?;
        loop {
            let mut bytes = Vec::new();
            let mut limited = (&mut self.stdout).take(MAX_FRAME as u64 + 1);
            use std::io::Read;
            let n = limited
                .read_until(0, &mut bytes)
                .map_err(|e| format!("proc-macro response failed: {e}"))?;
            if n == 0 || n > MAX_FRAME {
                return Err("proc-macro server closed or exceeded its response limit".into());
            }
            let response: Message = postcard::from_bytes_cobs(&mut bytes)
                .map_err(|e| format!("invalid proc-macro protocol response: {e}"))?;
            match response {
                Message::Response(response) => return Ok(response),
                Message::SubRequest(request) => {
                    let Some(codec) = codec else {
                        return Err("unexpected proc-macro source query".into());
                    };
                    self.send(Message::SubResponse(codec.callback(request)?))?;
                }
                _ => return Err("unexpected proc-macro message".into()),
            }
        }
    }
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod tests {
    use super::*;
    use flux_syntax::{LexedStr, TextRange, TokenOrigin};
    #[test]
    fn real_serde_derive_expansions_parse() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new("cargo")
            .current_dir(&repo)
            .args([
                "build",
                "--offline",
                "--locked",
                "-p",
                "serde_derive",
                "--message-format=json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let artifact = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "serde_derive")
            .unwrap();
        let path = artifact["filenames"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|s| s.as_str())
            .find(|s| s.ends_with(".dylib"))
            .unwrap();
        let source = artifact["target"]["src_path"].as_str().unwrap();
        let rustc = Command::new("rustc").arg("-vV").output().unwrap();
        let mut cfg = flux_hir::CrateCfg::default();
        cfg.names.push(Symbol::intern("proc_macro"));
        cfg.values = artifact["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                (
                    Symbol::intern("feature"),
                    Symbol::intern(f.as_str().unwrap()),
                )
            })
            .collect();
        let macros = exports(
            &String::from_utf8(rustc.stdout).unwrap(),
            Path::new(path),
            Path::new(source),
            cfg,
        )
        .unwrap();
        let text = r#"#[serde(rename="Record")] struct Record<'a> { #[serde(rename="renamed")] value: &'a str }"#;
        let mut db = flux_hir::SourceDb::new();
        let root = db.add("serde-input.rs", text);
        let lexed = LexedStr::new(text);
        let tokens = (0..lexed.len())
            .filter(|&i| !lexed.kind(i).is_trivia())
            .map(|i| ProcMacroToken {
                edition: flux_syntax::Edition::E2024,
                kind: lexed.kind(i),
                text: lexed.token_text(i).into(),
                joint: i + 1 < lexed.len() && !lexed.kind(i + 1).is_trivia(),
                origin: TokenOrigin {
                    range: lexed.token_range(i),
                    group: 1,
                    ctxt: 0,
                },
            })
            .collect::<Vec<_>>();
        for name in ["Serialize", "Deserialize"] {
            let mac = macros.iter().find(|m| m.name.as_str() == name).unwrap();
            assert!(mac.helpers.iter().flatten().any(|s| s.as_str() == "serde"));
            let output = mac
                .expander
                .expand(ProcMacroInput {
                    invocation_crate: 1,
                    tokens: tokens.clone(),
                    attributes: None,
                    source: db.shared(root),
                    call: TextRange::new(0, text.len() as u32),
                    range_base: 0,
                    mixed_context: 42,
                    environment: std::collections::BTreeMap::new(),
                    current_dir: repo.clone(),
                })
                .unwrap();
            let mut text = String::new();
            for token in output {
                text.push_str(&token.text);
                if !token.joint {
                    text.push(' ');
                }
            }
            let parsed = flux_syntax::parse_source_file(&text);
            assert!(
                parsed.errors().is_empty(),
                "{name}: {:?}\n{text}",
                parsed.errors()
            );
            assert!(text.contains("renamed"), "serde helper lost: {text}");
            assert!(text.contains(name), "wrong derive output: {text}");
        }
    }
}

#[cfg(all(test, target_os = "macos", target_arch = "aarch64"))]
mod lifecycle_tests {
    use super::*;
    use std::collections::BTreeMap;
    fn input(id: u32, environment: BTreeMap<String, String>) -> ProcMacroInput {
        let mut db = flux_hir::SourceDb::new();
        let root = db.add("host-input.rs", "");
        ProcMacroInput {
            invocation_crate: id,
            tokens: Vec::new(),
            attributes: None,
            source: db.shared(root),
            call: flux_syntax::TextRange::empty(0),
            range_base: 0,
            mixed_context: 42,
            environment,
            current_dir: std::env::current_dir().unwrap(),
        }
    }
    fn text(tokens: Vec<ProcMacroToken>) -> String {
        tokens
            .iter()
            .map(|t| t.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }
    #[test]
    fn state_environment_and_timeout_are_scoped() {
        let root =
            std::env::temp_dir().join(format!("flux-proc-host-lifecycle-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("host.rs");
        let dylib = root.join("liblifecycle.dylib");
        std::fs::write(&source,r#"
extern crate proc_macro;
use proc_macro::TokenStream;
static COUNT:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
#[proc_macro] pub fn counter(_:TokenStream)->TokenStream{COUNT.fetch_add(1,std::sync::atomic::Ordering::SeqCst).to_string().parse().unwrap()}
#[proc_macro] pub fn environment(_:TokenStream)->TokenStream{format!("{:?}",std::env::var("PM_TEST_VALUE").unwrap()).parse().unwrap()}
#[proc_macro] pub fn logs(_:TokenStream)->TokenStream{println!("macro log");"0".parse().unwrap()}
#[proc_macro] pub fn hang(_:TokenStream)->TokenStream{
    std::fs::write(std::env::var("PM_TEST_PID").unwrap(),std::process::id().to_string()).unwrap();
    loop{std::thread::sleep(std::time::Duration::from_millis(10));}
}
"#).unwrap();
        let built = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=proc-macro"])
            .arg(&source)
            .arg("-o")
            .arg(&dylib)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let version = Command::new("rustc").arg("-vV").output().unwrap();
        let mut cfg = flux_hir::CrateCfg::default();
        cfg.names.push(Symbol::intern("proc_macro"));
        let exports = exports(
            &String::from_utf8(version.stdout).unwrap(),
            &dylib,
            &source,
            cfg,
        )
        .unwrap();
        let counter = &exports
            .iter()
            .find(|m| m.name.as_str() == "counter")
            .unwrap()
            .expander;
        assert_eq!(
            text(counter.expand(input(1, BTreeMap::new())).unwrap()),
            "0"
        );
        assert_eq!(
            text(counter.expand(input(1, BTreeMap::new())).unwrap()),
            "1"
        );
        assert_eq!(
            text(counter.expand(input(2, BTreeMap::new())).unwrap()),
            "0"
        );
        let logs = &exports
            .iter()
            .find(|m| m.name.as_str() == "logs")
            .unwrap()
            .expander;
        let error = logs.expand(input(99, BTreeMap::new())).unwrap_err();
        assert!(error.contains("protocol response"), "{error}");
        assert!(logs.expand(input(99, BTreeMap::new())).is_err());
        let before = std::env::var_os("PM_TEST_VALUE");
        let env = &exports
            .iter()
            .find(|m| m.name.as_str() == "environment")
            .unwrap()
            .expander;
        let mut workers = Vec::new();
        for id in 10..14 {
            let env = env.clone();
            workers.push(std::thread::spawn(move || {
                let value = format!("crate-{id}");
                for _ in 0..3 {
                    assert_eq!(
                        text(
                            env.expand(input(
                                id,
                                BTreeMap::from([("PM_TEST_VALUE".into(), value.clone())])
                            ))
                            .unwrap()
                        ),
                        format!("{value:?}")
                    );
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(std::env::var_os("PM_TEST_VALUE"), before);
        let sysroot = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .unwrap();
        let server = Path::new(String::from_utf8(sysroot.stdout).unwrap().trim())
            .join("libexec/rust-analyzer-proc-macro-srv");
        let service = Service::start(server, Duration::from_millis(1500));
        assert!(matches!(
            service.submit(Task::List(dylib.clone())).unwrap(),
            Reply::List(_)
        ));
        let pid_file = root.join("pid");
        let _ = std::fs::remove_file(&pid_file);
        let started = std::time::Instant::now();
        let error = service
            .submit(Task::Expand {
                dylib: dylib.clone(),
                name: "hang".into(),
                input: Box::new(input(
                    1,
                    BTreeMap::from([("PM_TEST_PID".into(), pid_file.to_str().unwrap().into())]),
                )),
            })
            .err()
            .expect("hanging macro must time out");
        assert!(error.contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
        let pid = std::fs::read_to_string(pid_file).unwrap();
        assert!(
            !Command::new("/bin/kill")
                .args(["-0", &pid])
                .output()
                .unwrap()
                .status
                .success(),
            "timed-out host still alive"
        );
        assert!(
            service.submit(Task::List(dylib)).is_err(),
            "terminated host must not restart with reset state"
        );
    }
    #[test]
    fn unsupported_toolchain_is_rejected_before_loading() {
        assert!(
            exports(
                "unknown compiler",
                Path::new("absent.dylib"),
                Path::new("absent.rs"),
                Default::default()
            )
            .unwrap_err()
            .contains("rustc commit")
        );
    }
}
