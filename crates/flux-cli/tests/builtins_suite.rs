//! Explicitly staged built-ins match the source path in fresh compiler processes.

mod flib_support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use flux_cli::flib::executable::{CheckMethod, ExecutableIdentity};
use flux_hir::flib::{Decode, Decoder, Encode, Encoder};
use flux_intern::Symbol;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

struct Staged {
    scratch: Scratch,
    executable: PathBuf,
    suite: PathBuf,
}

impl Staged {
    fn new(native: bool) -> Self {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let scratch = Scratch::new(repository.join("out/builtins-suite-tests"));
        let executable = scratch.path().join("flux");
        std::fs::copy(env!("CARGO_BIN_EXE_flux"), &executable).unwrap();
        let suite = executable.with_file_name("flux-builtins.flib");
        let staged = Self {
            scratch,
            executable,
            suite,
        };
        let mut command = staged.command();
        command.arg("build-builtins");
        if native {
            command.arg("--native-object");
        }
        success(&flib_support::run(&mut command, "stage-builtins"));
        assert!(staged.suite.is_file());
        let before = std::fs::metadata(&staged.suite).unwrap();
        let contents = std::fs::read(&staged.suite).unwrap();
        let mut repeated = staged.command();
        repeated.arg("build-builtins");
        if native {
            repeated.arg("--native-object");
        }
        success(&flib_support::run(&mut repeated, "repeat-stage-builtins"));
        let after = std::fs::metadata(&staged.suite).unwrap();
        assert_eq!(
            after.modified().unwrap(),
            before.modified().unwrap(),
            "unchanged staging must preserve the suite mtime"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                after.ino(),
                before.ino(),
                "unchanged staging must preserve the suite inode"
            );
        }
        assert_eq!(std::fs::read(&staged.suite).unwrap(), contents);

        staged
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .current_dir(self.scratch.path())
            .env_remove("FLUX_BUILTINS_SOURCE")
            .env_remove("FLUX_BUILTINS_MIR_ONLY")
            .env("FLUX_BUILTINS_TRACE", "1");
        command
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.scratch.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    fn compiler(&self, source: bool, command: &str, file: &Path) -> Output {
        let mut process = self.command();
        process.arg(command).arg(file);
        if source {
            process.env("FLUX_BUILTINS_SOURCE", "1");
        }
        flib_support::run(&mut process, command)
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn without_trace(output: &[u8]) -> String {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| !line.starts_with("flux builtins:"))
        .map(|line| format!("{line}\n"))
        .collect()
}

fn normalize_runtime(output: &[u8]) -> String {
    let mut output = String::from_utf8(output.to_vec()).unwrap();
    // Each fresh process reports its own thread ID. Keep the panic's exact
    // message and source path, line and column while removing that identity.
    if let Some(start) = output.find("thread 'main' (") {
        let start = start + "thread 'main' (".len();
        if let Some(length) = output[start..].find(')') {
            assert!(
                output[start..start + length]
                    .bytes()
                    .all(|b| b.is_ascii_digit())
            );
            output.replace_range(start..start + length, "thread-id");
        }
    }
    output
}

fn trace(output: &Output, expected: &str) {
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn staged_metadata_native_object_and_mir_match_source_compilation() {
    let staged = Staged::new(true);
    let (suite, method) = flux_cli::builtins::load(&staged.suite, &staged.executable)
        .unwrap()
        .unwrap();
    assert_eq!(method, CheckMethod::Stat);
    assert!(suite.items.has_artifact_builtins());
    assert!(
        suite
            .object
            .as_ref()
            .is_some_and(|object| !object.is_empty())
    );
    assert!(
        suite
            .db
            .files()
            .all(|(_, file)| file.parse.tree().text().is_empty())
    );
    for (id, function) in suite.items.functions.iter() {
        assert_eq!(suite.mir.bodies.get(id).is_some(), function.has_body);
    }
    for (id, konst) in suite.items.consts.iter() {
        assert_eq!(suite.mir.consts.get(id).is_some(), konst.has_body);
    }
    let mut db = suite.db.fork();
    let root = db.add("validate.rs", "");
    let hir = flux_hir::lower_source_on(
        &mut db,
        suite.items.clone(),
        root,
        Symbol::intern("validation"),
        Vec::new(),
    );
    let errors = flux_mir::validate::validate_file(&hir, &suite.sigs, &suite.mir);
    assert!(errors.is_empty(), "{}", errors.join("\n"));

    let programs = [
        (
            "behavior",
            r#"
const N: usize = 3u32.count_ones() as usize + 1;
trait Read { fn read(&self) -> u32; }
struct Packet<const M: usize>([u8; M]);
impl<const M: usize> Read for Packet<M> { fn read(&self) -> u32 { self.0.len() as u32 } }
fn identity<const M: usize>(value: [u8; M]) -> [u8; M] { value }
#[track_caller]
fn caller_line() -> u32 { std::panic::Location::caller().line() }
fn main() {
    let packet = Packet(identity([1u8; N]));
    let reader: &dyn Read = &packet;
    println!("{} {}", reader.read(), caller_line());
}
"#,
        ),
        (
            "panic",
            r#"
#[track_caller]
fn fail() { panic!("staged built-in panic"); }
fn main() { fail(); }
"#,
        ),
    ];
    for (name, text) in programs {
        let file = staged.write(&format!("{name}.rs"), text);
        for optimized in [false, true] {
            let mut expected: Option<Output> = None;
            for (label, source, mir_only) in [
                ("source", true, false),
                ("suite", false, false),
                ("mir-only", false, true),
            ] {
                let executable = staged
                    .scratch
                    .path()
                    .join(format!("{name}-{optimized}-{label}"));
                let mut command = staged.command();
                command.arg("build").arg(&file).arg("-o").arg(&executable);
                if source {
                    command.env("FLUX_BUILTINS_SOURCE", "1");
                }
                if mir_only {
                    command.env("FLUX_BUILTINS_MIR_ONLY", "1");
                }
                if optimized {
                    command.arg("-O");
                }
                let built = flib_support::run(&mut command, "build-suite-program");
                success(&built);
                trace(
                    &built,
                    if source {
                        "source (forced)"
                    } else {
                        "flux builtins: suite"
                    },
                );
                let actual = flib_support::run(&mut Command::new(executable), "run-suite-program");
                if let Some(expected) = &expected {
                    assert_eq!(&actual.stdout, &expected.stdout);
                    assert_eq!(
                        normalize_runtime(&actual.stderr),
                        normalize_runtime(&expected.stderr)
                    );
                    assert_eq!(actual.status.code(), expected.status.code());
                } else {
                    if name == "panic" {
                        assert!(!actual.status.success());
                        assert!(
                            String::from_utf8_lossy(&actual.stderr)
                                .contains("staged built-in panic")
                        );
                    } else {
                        success(&actual);
                    }
                    expected = Some(actual);
                }
            }
        }
        let source = staged.compiler(true, "mir", &file);
        let loaded = staged.compiler(false, "mir", &file);
        success(&source);
        success(&loaded);
        assert_eq!(source.stdout, loaded.stdout);
        assert_eq!(without_trace(&source.stderr), without_trace(&loaded.stderr));
    }
    let invalid = staged.write("invalid.rs", "fn main() { let value: u32 = true; }");
    let source = staged.compiler(true, "mir", &invalid);
    let loaded = staged.compiler(false, "mir", &invalid);
    assert!(!source.status.success());
    assert!(!loaded.status.success());
    assert!(String::from_utf8_lossy(&source.stderr).contains("mismatched types"));
    assert_eq!(source.stdout, loaded.stdout);
    assert_eq!(without_trace(&source.stderr), without_trace(&loaded.stderr));
}

#[test]
fn suite_checksum_stat_cache_rechecks_a_replaced_file() {
    let staged = Staged::new(false);
    let file = staged.write("main.rs", "fn main() {}\n");
    assert!(staged.suite.with_extension("flib.stamp").is_file());
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "checksum=Stat");
    let replacement = staged.suite.with_extension("replacement");
    std::fs::copy(&staged.suite, &replacement).unwrap();
    std::fs::rename(&replacement, &staged.suite).unwrap();
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "checksum=Hash");
}

#[test]
fn executable_identity_fallback_and_suite_corruption_are_distinct() {
    let staged = Staged::new(false);
    let file = staged.write("main.rs", "fn main() {}\n");
    let original = std::fs::read(&staged.suite).unwrap();
    let modified = SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&staged.executable)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let (_, method) = flux_cli::builtins::load(&staged.suite, &staged.executable)
        .unwrap()
        .unwrap();
    assert_eq!(
        method,
        CheckMethod::Hash,
        "mtime-only changes must recheck content"
    );
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "identity=Hash");
    std::fs::remove_file(&staged.suite).unwrap();
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "source (suite missing or compiler changed)");

    let other = staged.write("different-compiler", "different executable content");
    let mut archive = flux_cli::flib::Archive::decode(&original).unwrap();
    let mut d = Decoder::new(&archive.metadata);
    let magic = d.read(8).unwrap().to_vec();
    let version = u32::decode(&mut d).unwrap();
    ExecutableIdentity::decode(&mut d).unwrap();
    let rest = d.read(d.remaining()).unwrap().to_vec();
    let mut e = Encoder::new();
    e.write(&magic);
    version.encode(&mut e).unwrap();
    ExecutableIdentity::capture(&other)
        .unwrap()
        .encode(&mut e)
        .unwrap();
    e.write(&rest);
    archive.metadata = e.into_bytes();
    std::fs::write(&staged.suite, archive.encode().unwrap()).unwrap();
    assert!(
        flux_cli::builtins::load(&staged.suite, &staged.executable)
            .unwrap()
            .is_none()
    );
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "source (suite missing or compiler changed)");

    let mut archive = flux_cli::flib::Archive::decode(&original).unwrap();
    *archive.metadata.last_mut().unwrap() ^= 1;
    std::fs::write(&staged.suite, archive.encode().unwrap()).unwrap();
    let output = staged.compiler(false, "mir", &file);
    assert!(!output.status.success());
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains(staged.suite.to_str().unwrap()), "{errors}");
    assert!(errors.contains("checksum mismatch"), "{errors}");
    assert!(
        errors.contains("run `flux build-builtins` again"),
        "{errors}"
    );
    let source = staged.compiler(true, "mir", &file);
    success(&source);
    trace(&source, "source (forced)");
}
