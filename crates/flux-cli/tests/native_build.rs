//! Native crate builds cache observed inputs and never select a rustc fallback.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
use support::scratch;
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

struct Fixture(scratch::Scratch);
impl Fixture {
    fn new() -> Self {
        let fixture = Self(scratch::Scratch::new(
            std::env::temp_dir().join("flux-native-build"),
        ));
        fixture.write(
            "tools/rustc",
            "#!/bin/sh\necho rustc >> \"$COMPILER_TRAP\"\nexit 94\n",
        );
        fixture.write(
            "tools/cargo",
            "#!/bin/sh\necho cargo >> \"$COMPILER_TRAP\"\nexit 93\n",
        );
        use std::os::unix::fs::PermissionsExt;
        for tool in ["rustc", "cargo"] {
            std::fs::set_permissions(
                fixture.path().join("tools").join(tool),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        fixture
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn build(&self, args: &[&str], value: &str) -> Output {
        let mut paths = vec![self.path().join("tools")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .args(["build", "--offline", "--manifest-path"])
            .arg(self.path().join("Cargo.toml"))
            .args(args)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("RUSTC", self.path().join("tools/rustc"))
            .env("CARGO", self.path().join("tools/cargo"))
            .env("CARGO_HOME", self.path().join("cache"))
            .env("COMPILER_TRAP", self.path().join("compiler-was-called"))
            .env("BUILD_VALUE", value);
        support::process::run(&mut command, self.path(), "build", Duration::from_secs(120))
            .completed()
    }
    fn success(&self, args: &[&str], value: &str) -> String {
        let output = self.build(args, value);
        support::oracle::success(&output, "native Flux crate build");
        assert!(!self.path().join("compiler-was-called").exists());
        String::from_utf8(output.stderr).unwrap()
    }
    fn run(&self, expected: &[u8]) {
        let output = Command::new(self.path().join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, "native executable");
        assert_eq!(output.stdout, expected);
    }
}

#[test]
fn diamond_build_reuses_inputs_and_recovers_from_failures() {
    let f = Fixture::new();
    f.write("Cargo.toml", "[workspace]\nmembers=['app','left','right','shared']\ndefault-members=['app']\nresolver='3'\n");
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nleft={path='../left'}\nright={path='../right'}\n");
    f.write("left/Cargo.toml", "[package]\nname='left'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nshared={path='../shared'}\n");
    f.write("right/Cargo.toml", "[package]\nname='right'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nrenamed={package='shared',path='../shared'}\n");
    f.write(
        "shared/Cargo.toml",
        "[package]\nname='shared'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("shared/src/lib.rs", "pub fn value()->u32 { 10 }\n");
    f.write(
        "left/src/lib.rs",
        "pub fn value()->u32 { shared::value()+1 }\n",
    );
    f.write("right/build.rs", "fn main(){let out=std::env::var(\"OUT_DIR\").unwrap();std::fs::write(format!(\"{out}/generated.rs\"),\"pub fn extra()->u32{2}\").unwrap();}\n");
    f.write("right/src/lib.rs", "include!(concat!(env!(\"OUT_DIR\"),\"/generated.rs\"));pub fn value()->u32 { renamed::value()+extra() }\n");
    f.write("app/build.rs", "fn main(){println!(\"cargo::rustc-env=BUILD_VALUE={}\",std::env::var(\"BUILD_VALUE\").unwrap());}\n");
    f.write(
        "app/src/main.rs",
        "fn main(){println!(\"{} {}\",left::value()+right::value(),env!(\"BUILD_VALUE\"));}\n",
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['left','right']\n[[package]]\nname='left'\nversion='0.1.0'\ndependencies=['shared']\n[[package]]\nname='right'\nversion='0.1.0'\ndependencies=['shared']\n[[package]]\nname='shared'\nversion='0.1.0'\n");
    let first = f.success(&["-j", "2"], "first");
    for name in ["shared", "left", "right", "app"] {
        assert!(first.contains(&format!("Compiling {name}")), "{first}");
    }
    f.run(b"23 first\n");
    let second = f.success(&["-j", "1"], "first");
    for name in ["shared", "left", "right", "app"] {
        assert!(second.contains(&format!("Fresh {name}")), "{second}");
    }
    f.run(b"23 first\n");
    // Rewriting identical contents must not rebuild, even though metadata changes.
    f.write("shared/src/lib.rs", "pub fn value()->u32 { 10 }\n");
    let touched = f.success(&["-j", "2"], "first");
    assert!(touched.contains("Fresh shared"), "{touched}");
    assert!(touched.contains("Fresh app"), "{touched}");
    let changed_env = f.success(&[], "second");
    assert!(changed_env.contains("Fresh shared"), "{changed_env}");
    assert!(changed_env.contains("Compiling app"), "{changed_env}");
    f.run(b"23 second\n");
    // A corrupted record or executable cannot authorize reuse.
    f.write("target/flux/debug/app.fresh.json", "incomplete record");
    assert!(f.success(&[], "second").contains("Compiling app"));
    f.write("target/flux/debug/app", "corrupt executable");
    assert!(f.success(&[], "second").contains("Compiling app"));
    f.run(b"23 second\n");
    f.write("shared/src/lib.rs", "pub fn value()->u32 { 20 }\n");
    let changed = f.success(&[], "second");
    for name in ["shared", "left", "right", "app"] {
        assert!(changed.contains(&format!("Compiling {name}")), "{changed}");
    }
    f.run(b"43 second\n");
    f.write("shared/src/lib.rs", "pub fn value()->u32 { missing }\n");
    let failed = f.build(&[], "second");
    assert!(!failed.status.success());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains("Compiling app"));
    f.write("shared/src/lib.rs", "pub fn value()->u32 { 30 }\n");
    f.success(&[], "second");
    f.run(b"63 second\n");
    f.success(&["-O"], "second");
    f.run(b"63 second\n");
    assert!(f.success(&["-O"], "second").contains("Fresh app"));
    assert!(f.success(&[], "second").contains("Fresh app"));
    let shared = std::fs::read_dir(f.path().join("target/flux/debug/deps"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("shared-")
                && path
                    .extension()
                    .is_some_and(|extension| extension == "flib")
        })
        .unwrap();
    std::fs::remove_file(shared).unwrap();
    let restored = f.success(&[], "second");
    assert!(restored.contains("Compiling shared"), "{restored}");
    assert!(restored.contains("Fresh left"), "{restored}");
    f.run(b"63 second\n");
    let library = if cfg!(target_os = "macos") {
        "System"
    } else {
        "c"
    };
    f.write("app/src/main.rs", &format!(
        "#[link(name={library:?})] unsafe extern \"C\" {{}} fn main(){{println!(\"native link\");}}\n"
    ));
    f.success(&[], "second");
    assert!(f.success(&[], "second").contains("Compiling app"));
    f.run(b"native link\n");
    let invalid = f.build(&["-j", "0"], "second");
    assert!(!invalid.status.success());
}

#[test]
fn default_flux_macro_host_with_dependency_rebuilds_consumers() {
    let f = Fixture::new();
    f.write("Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='macros'}\n");
    f.write("macros/Cargo.toml", "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\nhelper={path='../helper'}\n");
    f.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("helper/src/lib.rs", "pub fn value()->u32{42}\n");
    f.write("macros/src/lib.rs", "extern crate proc_macro;use proc_macro::TokenStream;#[proc_macro] pub fn answer(_:TokenStream)->TokenStream{helper::value().to_string().parse().unwrap()}\n");
    f.write(
        "src/main.rs",
        "fn main(){println!(\"{}\",macros::answer!());}\n",
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['macros']\n[[package]]\nname='macros'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n");
    f.success(&["-j", "2"], "first");
    f.run(b"42\n");
    let second = f.success(&["-j", "2"], "first");
    assert!(second.contains("Compiling app"), "{second}");
    f.write("helper/src/lib.rs", "pub fn value()->u32{73}\n");
    f.success(&[], "first");
    f.run(b"73\n");
}

#[test]
fn workspace_defaults_build_libraries_and_all_enabled_binaries_in_separate_profiles() {
    let f = Fixture::new();
    f.write("Cargo.toml", "[workspace]\nmembers=['app','helper','excluded']\ndefault-members=['app','helper']\nresolver='3'\n");
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\ndefault-run='primary'\n[[bin]]\nname='primary'\npath='src/main.rs'\n[features]\nextra=[]\n[[bin]]\nname='gated'\npath='gated.rs'\nrequired-features=['extra']\n");
    f.write("app/src/lib.rs", "pub fn value()->u32{42}\n");
    f.write("app/build.rs", "fn main(){println!(\"cargo::rustc-env=TARGET_PROFILE={}:{}:{}\",std::env::var(\"PROFILE\").unwrap(),std::env::var(\"OPT_LEVEL\").unwrap(),std::env::var(\"DEBUG\").unwrap());}\n");
    f.write("app/src/main.rs", "fn main(){println!(\"{} {} {}\",app::value(),cfg!(debug_assertions),env!(\"TARGET_PROFILE\"));}\n");
    f.write(
        "app/src/bin/second.rs",
        "fn main(){println!(\"second\");}\n",
    );
    f.write(
        "app/gated.rs",
        "compile_error!(\"disabled binary should be skipped\");\n",
    );
    f.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("helper/src/lib.rs", "pub fn helper()->u32{7}\n");
    f.write(
        "excluded/Cargo.toml",
        "[package]\nname='excluded'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write(
        "excluded/src/main.rs",
        "compile_error!(\"excluded member\");\n",
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\n[[package]]\nname='helper'\nversion='0.1.0'\n[[package]]\nname='excluded'\nversion='0.1.0'\n");
    let built = f.success(&[], "");
    assert_eq!(built.matches("Compiling app\n").count(), 1, "{built}");
    for file in ["primary", "second", "libapp.flib", "libhelper.flib"] {
        assert!(
            f.path().join("target/flux/debug").join(file).is_file(),
            "{file}"
        );
    }
    assert!(!f.path().join("target/flux/debug/excluded").exists());
    let run = |profile: &str| {
        let result = Command::new(f.path().join("target/flux").join(profile).join("primary"))
            .output()
            .unwrap();
        support::oracle::success(&result, "workspace binary");
        result.stdout
    };
    assert_eq!(run("debug"), b"42 true debug:0:true\n");
    f.success(&["--release"], "");
    let release = Command::new(f.path().join("target/flux/release/primary"))
        .output()
        .unwrap();
    support::oracle::success(&release, "release binary");
    assert_eq!(release.stdout, b"42 false release:3:false\n");
    assert_eq!(run("debug"), b"42 true debug:0:true\n");
    assert!(!f.build(&["--workspace"], "").status.success());
    let explicit = f.success(&["-p", "helper", "--lib"], "");
    assert!(!explicit.contains("Compiling app"));
}

#[test]
fn arena_library_export_dispatch_terminates() {
    let f = Fixture::new();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-arena/src/lib.rs");
    let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
    command
        .args(["build", "--crate-type", "lib"])
        .arg(source)
        .arg("-o")
        .arg(f.path().join("arena.flib"));
    let output = support::process::run(
        &mut command,
        f.path(),
        "arena-library",
        Duration::from_secs(60),
    )
    .completed();
    support::oracle::success(&output, "generic library export dispatch");
    assert!(f.path().join("arena.flib").is_file());
}
