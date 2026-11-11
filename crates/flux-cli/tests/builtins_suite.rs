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
        std::fs::copy(env!("CARGO_BIN_EXE_fluxc"), &executable).unwrap();
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
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unwind_suite_native_object_and_mir_match_sources() {
    let staged = Staged::new(true);
    let suite_path = staged
        .executable
        .with_file_name("flux-builtins-unwind.flib");
    let file = staged.write(
        "unwind.rs",
        r#"
struct Cleanup;
impl Drop for Cleanup {fn drop(&mut self){assert!(std::thread::panicking());println!("cleanup");}}
fn fail<T>(_:T){let _value=Cleanup;std::panic::panic_any(String::from("owned panic"));}
fn main(){
    assert!(cfg!(panic="unwind"));
    std::panic::set_hook(Box::new(|_|{}));
    let cell=std::cell::RefCell::new(7u32);
    let projected=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let _:std::cell::Ref<'_,u32>=std::cell::Ref::map(cell.borrow(),|_|panic!("projection"));
    }));
    assert!(projected.is_err());assert!(cell.try_borrow_mut().is_ok());
    let value=std::panic::catch_unwind(||fail(42u32)).unwrap_err();
    assert_eq!(*value.downcast::<String>().unwrap(),"owned panic");
    let result=std::thread::spawn(||std::panic::panic_any(19usize)).join().unwrap_err();
    assert_eq!(*result.downcast::<usize>().unwrap(),19);
    println!("caught");
}
"#,
    );
    for optimized in [false, true] {
        let mut stage = staged.command();
        stage.args(["build-builtins", "--native-object", "-C", "panic=unwind"]);
        if optimized {
            stage.arg("-O");
        }
        success(&flib_support::run(&mut stage, "stage-unwind-builtins"));
        let (suite, _) = flux_cli::builtins::load(&suite_path, &staged.executable)
            .unwrap()
            .unwrap();
        assert_eq!(suite.mir.panic_strategy, flux_mir::PanicStrategy::Unwind);
        assert!(suite.object.is_some());
        for (label, source, mir_only) in [
            ("source", true, false),
            ("suite", false, false),
            ("mir", false, true),
        ] {
            let binary = staged
                .scratch
                .path()
                .join(format!("unwind-{optimized}-{label}"));
            let mut build = staged.command();
            build
                .arg("build")
                .arg(&file)
                .arg("-o")
                .arg(&binary)
                .args(["-C", "panic=unwind"]);
            if optimized {
                build.arg("-O");
            }
            if source {
                build.env("FLUX_BUILTINS_SOURCE", "1");
            }
            if mir_only {
                build.env("FLUX_BUILTINS_MIR_ONLY", "1");
            }
            let output = flib_support::run(&mut build, "build-unwind-suite");
            success(&output);
            trace(
                &output,
                if source {
                    "source (forced)"
                } else {
                    "flux builtins: suite"
                },
            );
            let output = flib_support::run(&mut Command::new(binary), "run-unwind-suite");
            success(&output);
            assert_eq!(output.stdout, b"cleanup\ncaught\n");
            assert!(
                output.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    let library = staged.write(
        "dependency.rs",
        r#"
struct Cleanup;
impl Drop for Cleanup {fn drop(&mut self){println!("artifact cleanup");}}
#[inline(never)] fn fail_inner(){let _guard=Cleanup;panic!("artifact payload");}
pub fn fail<T>(_:T){fail_inner();}
"#,
    );
    for strategy in ["abort", "unwind"] {
        let artifact = staged
            .scratch
            .path()
            .join(format!("dependency-{strategy}.flib"));
        let mut build = staged.command();
        build
            .arg("build")
            .arg(&library)
            .args([
                "--crate-type",
                "lib",
                "--crate-name",
                "dependency",
                "-O",
                "-C",
            ])
            .arg(format!("panic={strategy}"))
            .arg("-o")
            .arg(artifact);
        success(&flib_support::run(&mut build, "build-strategy-artifact"));
    }
    std::fs::remove_file(&library).unwrap();
    let consumer = staged.write(
        "consumer.rs",
        r#"
fn main(){
    #[cfg(panic="unwind")] {
        std::panic::set_hook(Box::new(|_|{}));
        let error=std::panic::catch_unwind(||dependency::fail(17u32)).unwrap_err();
        assert_eq!(*error.downcast_ref::<&str>().unwrap(),"artifact payload");
    }
    println!("artifact passed");
}
"#,
    );
    for artifact_strategy in ["abort", "unwind"] {
        for strategy in ["abort", "unwind"] {
            let binary = staged
                .scratch
                .path()
                .join(format!("artifact-{artifact_strategy}-{strategy}"));
            let artifact = staged
                .scratch
                .path()
                .join(format!("dependency-{artifact_strategy}.flib"));
            let mut build = staged.command();
            build
                .arg("build")
                .arg(&consumer)
                .args(["-O", "--extern"])
                .arg(format!("dependency={}", artifact.display()))
                .args(["-C"])
                .arg(format!("panic={strategy}"))
                .arg("-o")
                .arg(&binary);
            let output = flib_support::run(&mut build, "load-strategy-artifact");
            if artifact_strategy == strategy {
                success(&output);
                let output = flib_support::run(&mut Command::new(binary), "run-strategy-artifact");
                success(&output);
                assert_eq!(
                    output.stdout,
                    if strategy == "unwind" {
                        &b"artifact cleanup\nartifact passed\n"[..]
                    } else {
                        &b"artifact passed\n"[..]
                    }
                );
            } else {
                assert!(!output.status.success());
                assert!(!binary.exists());
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains("panic strategy mismatch"),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    // One driver process switches to abort for host compilation, then restores
    // its unwind target suite before loading dependency state and checking cfg.
    let manifest = staged.write("Cargo.toml", "[workspace]\n[package]\nname='staged-app'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='staged-app'\npath='package.rs'\n[profile.dev]\npanic='unwind'\n");
    staged.write(
        "Cargo.lock",
        "version=4\n[[package]]\nname='staged-app'\nversion='0.1.0'\n",
    );
    staged.write("build.rs", r#"fn main(){println!("cargo:rustc-env=TARGET_PANIC={}",std::env::var("CARGO_CFG_PANIC").unwrap());}"#);
    staged.write("package.rs", r#"fn main(){assert!(cfg!(panic="unwind"));assert_eq!(env!("TARGET_PANIC"),"unwind");std::panic::set_hook(Box::new(|_|{}));assert!(std::panic::catch_unwind(||panic!("staged package")).is_err());println!("staged package passed");}"#);
    let mut build = staged.command();
    build
        .args([
            "package-driver",
            "build",
            "--offline",
            "--macro-host-compiler",
            "flux",
            "-j2",
            "--manifest-path",
        ])
        .arg(manifest);
    let output = flib_support::run(&mut build, "build-staged-unwind-package");
    success(&output);
    assert!(String::from_utf8_lossy(&output.stderr).contains("flux-builtins-unwind.flib"));
    let output = flib_support::run(
        &mut Command::new(staged.scratch.path().join("target/flux/debug/staged-app")),
        "run-staged-unwind-package",
    );
    success(&output);
    assert_eq!(output.stdout, b"staged package passed\n");
    // A valid abort suite stored at the unwind path must fail before compilation.
    std::fs::copy(&staged.suite, &suite_path).unwrap();
    let mut build = staged.command();
    build.arg("build").arg(&file).args(["-C", "panic=unwind"]);
    let output = flib_support::run(&mut build, "reject-wrong-strategy-suite");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("built-in panic strategy mismatch"));
}

#[test]
fn staged_metadata_native_object_and_mir_match_source_compilation() {
    let staged = Staged::new(true);
    let (suite, method) = flux_cli::builtins::load(&staged.suite, &staged.executable)
        .unwrap()
        .unwrap();
    assert_eq!(method, CheckMethod::Stat);
    assert!(suite.items.has_artifact_builtins());
    let unwinder: std::collections::HashSet<_> = suite
        .items
        .functions
        .iter()
        .filter(|(_, function)| {
            matches!(
                function.name.as_str(),
                "_Unwind_RaiseException" | "_Unwind_DeleteException" | "__gxx_personality_v0"
            )
        })
        .map(|(id, _)| id)
        .collect();
    assert_eq!(unwinder.len(), 3);
    for body in suite
        .mir
        .bodies
        .iter()
        .map(|(_, body)| body)
        .chain(suite.mir.closures.values())
    {
        for block in body.blocks.values() {
            if let flux_mir::TerminatorKind::Call { func, .. } = block.terminator.kind {
                assert!(
                    !unwinder.contains(&func),
                    "abort builtins must not call the platform unwinder"
                );
            }
        }
    }
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
    // That load verified the replacement and recorded its stat fields.
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "checksum=Stat");
    let repair = flib_support::run(
        staged.command().arg("build-builtins"),
        "repair-replaced-suite-stamp",
    );
    success(&repair);
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "checksum=Stat");
    std::fs::remove_file(staged.suite.with_extension("flib.stamp")).unwrap();
    let before = std::fs::metadata(&staged.suite).unwrap();
    let repair = flib_support::run(
        staged.command().arg("build-builtins"),
        "repair-missing-suite-stamp",
    );
    success(&repair);
    assert!(staged.suite.with_extension("flib.stamp").is_file());
    assert_eq!(
        std::fs::metadata(&staged.suite)
            .unwrap()
            .modified()
            .unwrap(),
        before.modified().unwrap()
    );
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "checksum=Stat");
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
    // The matching hash records the executable's new stat fields, so later
    // processes skip the content hash until the executable changes again.
    let (_, method) = flux_cli::builtins::load(&staged.suite, &staged.executable)
        .unwrap()
        .unwrap();
    assert_eq!(method, CheckMethod::Stat);
    let output = staged.compiler(false, "mir", &file);
    success(&output);
    trace(&output, "identity=Stat");
    trace(&output, "checksum=Stat");
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
