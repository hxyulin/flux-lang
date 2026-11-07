//! Locked fetching populates an empty cache without Cargo or dependency resolution.
#![cfg(unix)]
mod support;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../out/locked-fetch/{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let f = Self(path.canonicalize().unwrap());
        f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrenamed={package=\"sample\",version=\"=1.0.0\"}\n");
        f.write(
            "src/main.rs",
            "fn main(){println!(\"{}\",renamed::value());}\n",
        );
        f.write(
            "tools/cargo",
            "#!/bin/sh\necho called > \"$CARGO_TRAP\"\nexit 93\n",
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            f.0.join("tools/cargo"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        f
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
        let path = std::env::join_paths(
            std::iter::once(self.0.join("tools"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        c.env("PATH", path)
            .env("CARGO", self.0.join("tools/cargo"))
            .env("CARGO_TRAP", self.0.join("cargo-called"))
            .env("CARGO_HOME", self.0.join("cache"))
            .env("FLUX_BUILTINS_TRACE", "1");
        c
    }
    fn fetch(&self, label: &str, args: &[&str]) -> Output {
        let output = support::process::run(
            self.command()
                .args(["fetch", "--locked", "--manifest-path"])
                .arg(self.0.join("Cargo.toml"))
                .args([
                    "--download-base",
                    &format!("file://{}", self.0.join("mirror").display()),
                ])
                .args(args)
                .env("RUSTC", self.0.join("rustc-must-not-run")),
            &self.0,
            label,
            Duration::from_secs(60),
        )
        .completed();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("flux builtins:"));
        assert!(!self.0.join("cargo-called").exists());
        output
    }
    fn lock(&self, checksum: &str) {
        self.write("Cargo.lock",&format!("version=4\n[[package]]\nname=\"app\"\nversion=\"0.1.0\"\ndependencies=[\"sample\"]\n[[package]]\nname=\"sample\"\nversion=\"1.0.0\"\nsource=\"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum=\"{checksum}\"\n"));
    }
    fn archive(&self, tar: &[u8]) -> String {
        let mut child = Command::new("gzip")
            .arg("-c")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(tar).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let checksum = hash(&output.stdout);
        let path = self.0.join("mirror/sample/1.0.0/download");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &output.stdout).unwrap();
        checksum
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn entry(tar: &mut Vec<u8>, name: &str, kind: u8, data: &[u8]) {
    let mut h = [0u8; 512];
    assert!(name.len() < 100);
    h[..name.len()].copy_from_slice(name.as_bytes());
    h[100..108].copy_from_slice(b"0000644\0");
    h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
    h[148..156].fill(b' ');
    h[156] = kind;
    h[257..263].copy_from_slice(b"ustar\0");
    let checksum: u64 = h.iter().map(|b| *b as u64).sum();
    h[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
    tar.extend_from_slice(&h);
    tar.extend_from_slice(data);
    tar.resize(tar.len().div_ceil(512) * 512, 0);
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn valid() -> Vec<u8> {
    let mut tar = Vec::new();
    entry(
        &mut tar,
        "sample-1.0.0/Cargo.toml",
        b'0',
        b"[package]\nname=\"sample\"\nversion=\"1.0.0\"\nedition=\"2024\"\n",
    );
    entry(
        &mut tar,
        "sample-1.0.0/src/lib.rs",
        b'0',
        b"pub fn value()->usize{47}\n",
    );
    tar.resize(tar.len() + 1024, 0);
    tar
}
fn success(output: &Output, label: &str) {
    support::oracle::success(output, label);
}

#[test]
fn registry_fetch_offline_reuse_and_cargo_free_build() {
    let f = Fixture::new("registry");
    let checksum = f.archive(&valid());
    f.lock(&checksum);
    let lock = std::fs::read(f.0.join("Cargo.lock")).unwrap();
    let missing = f.fetch("missing", &["--offline"]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("offline source missing"));
    success(&f.fetch("download", &[]), "download locked registry source");
    let source =
        f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0");
    let metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source.join(".cargo-checksum.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["package"], checksum);
    assert_eq!(
        metadata["files"]["src/lib.rs"],
        hash(b"pub fn value()->usize{47}\n")
    );
    // An extracted source tree and a cached archive each work with the mirror unavailable.
    std::fs::rename(f.0.join("mirror"), f.0.join("mirror-unavailable")).unwrap();
    success(&f.fetch("reuse", &["--offline"]), "reuse extracted source");
    std::fs::remove_dir_all(&source).unwrap();
    success(
        &f.fetch("unpack-offline", &["--offline"]),
        "extract cached archive offline",
    );
    if cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        let built = support::process::run(
            f.command()
                .args(["build", "--offline", "--manifest-path"])
                .arg(f.0.join("Cargo.toml")),
            &f.0,
            "native-build",
            Duration::from_secs(90),
        )
        .completed();
        success(&built, "build newly fetched dependency");
        let output = support::process::run(
            &mut Command::new(f.0.join("target/flux/debug/app")),
            &f.0,
            "native-run",
            Duration::from_secs(10),
        )
        .completed();
        success(&output, "run fetched dependency");
        assert_eq!(output.stdout, b"47\n");
    }
    assert_eq!(std::fs::read(f.0.join("Cargo.lock")).unwrap(), lock);
    assert!(!f.0.join("cargo-called").exists());
}
#[test]
fn checksum_and_archive_errors_do_not_publish_sources() {
    let cases = [
        ("checksum", None),
        ("parent", Some("sample-1.0.0/../escaped")),
        ("absolute", Some("/tmp/flux-fetch-escape")),
        ("other-root", Some("different-1.0.0/file")),
        ("backslash", Some("sample-1.0.0/..\\escaped")),
        ("symlink", Some("sample-1.0.0/link")),
        ("hardlink", Some("sample-1.0.0/link")),
        ("reserved", Some("sample-1.0.0/.cargo-ok")),
        ("duplicate", Some("sample-1.0.0/src/lib.rs")),
    ];
    for (name, path) in cases {
        let f = Fixture::new(name);
        let mut tar = valid();
        if let Some(path) = path {
            tar.truncate(tar.len() - 1024);
            entry(
                &mut tar,
                path,
                if name == "symlink" {
                    b'2'
                } else if name == "hardlink" {
                    b'1'
                } else {
                    b'0'
                },
                b"bad",
            );
            tar.resize(tar.len() + 1024, 0);
        }
        let checksum = f.archive(&tar);
        let bad = "0".repeat(64);
        f.lock(if name == "checksum" { &bad } else { &checksum });
        let result = f.fetch("fetch", &[]);
        assert!(!result.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains(if name == "checksum" {
                "checksum mismatch"
            } else {
                "archive"
            }),
            "{name}: {stderr}"
        );
        assert!(
            !f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0")
                .exists()
        );
        assert!(
            !f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/escaped")
                .exists()
        );
        for parent in [
            "cache/registry/cache/index.crates.io-1949cf8c6b5b557f",
            "cache/registry/src/index.crates.io-1949cf8c6b5b557f",
        ] {
            let parent = f.0.join(parent);
            if parent.exists() {
                assert!(std::fs::read_dir(parent).unwrap().all(|e| {
                    !e.unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".flux-fetch-")
                }));
            }
        }
    }
}
#[test]
fn exact_git_commit_workspace_and_offline_checkout() {
    let f = Fixture::new("git");
    f.write(
        "git-origin/Cargo.toml",
        "[workspace]\nmembers=[\"helper\"]\n[workspace.package]\nversion=\"1.0.0\"\n",
    );
    f.write(
        "git-origin/helper/Cargo.toml",
        "[package]\nname=\"helper\"\nversion.workspace=true\nedition=\"2024\"\n",
    );
    f.write(
        "git-origin/helper/src/lib.rs",
        "pub fn value()->usize{19}\n",
    );
    let origin = f.0.join("git-origin");
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "first",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&origin)
            .args(args)
            .output()
            .unwrap();
        success(&output, "create git fixture");
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&origin)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    success(&output, "git commit");
    let commit = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    // The locked commit is deliberately not the branch tip.
    f.write(
        "git-origin/helper/src/lib.rs",
        "pub fn value()->usize{99}\n",
    );
    let output = Command::new("git")
        .arg("-C")
        .arg(&origin)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qam",
            "second",
        ])
        .output()
        .unwrap();
    success(&output, "advance git fixture");
    let url = format!("file://{}", origin.display());
    f.write("Cargo.toml",&format!("[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nrenamed={{package=\"helper\",git=\"{url}\",rev=\"{commit}\"}}\n"));
    f.write("Cargo.lock",&format!("version=4\n[[package]]\nname=\"app\"\nversion=\"0.1.0\"\ndependencies=[\"helper\"]\n[[package]]\nname=\"helper\"\nversion=\"1.0.0\"\nsource=\"git+{url}?rev={commit}#{commit}\"\n"));
    let lock = std::fs::read(f.0.join("Cargo.lock")).unwrap();
    let missing = f.fetch("missing", &["--offline"]);
    assert!(!missing.status.success());
    success(&f.fetch("download", &[]), "fetch pinned git checkout");
    std::fs::rename(&origin, f.0.join("git-offline")).unwrap();
    success(
        &f.fetch("offline", &["--offline"]),
        "reuse git checkout offline",
    );
    let plan = f.0.join("plan.json");
    let output = support::process::run(
        f.command()
            .args(["plan", "--manifest-path"])
            .arg(f.0.join("Cargo.toml"))
            .arg("--plan")
            .arg(&plan),
        &f.0,
        "plan",
        Duration::from_secs(30),
    )
    .completed();
    success(&output, "plan fetched git source");
    let plan: serde_json::Value = serde_json::from_slice(&std::fs::read(plan).unwrap()).unwrap();
    let helper = plan["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["target"]["name"] == "helper")
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(helper["target"]["root"].as_str().unwrap()).unwrap(),
        "pub fn value()->usize{19}\n"
    );
    assert_eq!(std::fs::read(f.0.join("Cargo.lock")).unwrap(), lock);
}

#[test]
fn http_redirects_and_interrupted_downloads() {
    use std::io::Read;
    use std::net::TcpListener;
    for broken in [false, true] {
        let f = Fixture::new(if broken { "http-broken" } else { "http" });
        let checksum = f.archive(&valid());
        f.lock(&checksum);
        let payload = std::fs::read(f.0.join("mirror/sample/1.0.0/download")).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut paths = Vec::new();
            while paths.len() < if broken { 1 } else { 2 } {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut request = [0; 4096];
                        let len = stream.read(&mut request).unwrap();
                        let text = String::from_utf8_lossy(&request[..len]);
                        let path = text.split_whitespace().nth(1).unwrap().to_owned();
                        if broken {
                            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 99999\r\nConnection: close\r\n\r\nshort").unwrap();
                        } else if path == "/sample/1.0.0/download" {
                            stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /payload\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                        } else {
                            assert_eq!(path, "/payload");
                            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len()).unwrap();
                            stream.write_all(&payload).unwrap();
                        }
                        paths.push(path);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "curl did not request the fixture"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("HTTP fixture failed: {e}"),
                }
            }
            paths
        });
        let output = support::process::run(
            f.command()
                .args(["fetch", "--locked", "--manifest-path"])
                .arg(f.0.join("Cargo.toml"))
                .args(["--download-base", &format!("http://{address}")]),
            &f.0,
            "http-fetch",
            Duration::from_secs(30),
        )
        .completed();
        let requests = server.join().unwrap();
        assert_eq!(requests[0], "/sample/1.0.0/download");
        if broken {
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("registry download failed"));
            assert!(
                !f.0.join(
                    "cache/registry/cache/index.crates.io-1949cf8c6b5b557f/sample-1.0.0.crate"
                )
                .exists()
            );
        } else {
            success(&output, "HTTP redirect download");
            assert_eq!(requests[1], "/payload");
        }
        assert!(!f.0.join("cargo-called").exists());
    }
}
#[test]
fn gnu_long_paths_pax_paths_and_header_validation() {
    for (name, pax_header) in [("gnu", false), ("pax", true)] {
        let f = Fixture::new(name);
        let long = format!("sample-1.0.0/{}", "long".repeat(40));
        let mut tar = valid();
        tar.truncate(tar.len() - 1024);
        if pax_header {
            let content = format!("path={long}\n");
            let mut len = content.len() + 2;
            loop {
                let actual = content.len() + len.to_string().len() + 1;
                if actual == len {
                    break;
                }
                len = actual;
            }
            entry(
                &mut tar,
                "PaxHeader",
                b'x',
                format!("{len} {content}").as_bytes(),
            );
        } else {
            entry(
                &mut tar,
                "././@LongLink",
                b'L',
                format!("{long}\0").as_bytes(),
            );
        }
        entry(&mut tar, "ignored-name", b'0', b"long path content");
        tar.resize(tar.len() + 1024, 0);
        let checksum = f.archive(&tar);
        f.lock(&checksum);
        success(&f.fetch("fetch", &[]), "long archive paths");
        assert_eq!(
            std::fs::read(
                f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f")
                    .join(long)
            )
            .unwrap(),
            b"long path content"
        );
    }
    for (name, mut tar) in [
        ("truncated", valid()[..600].to_vec()),
        ("header-checksum", valid()),
    ] {
        if name == "header-checksum" {
            tar[0] ^= 1;
        }
        let f = Fixture::new(name);
        let checksum = f.archive(&tar);
        f.lock(&checksum);
        let output = f.fetch("fetch", &[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("archive")
                || String::from_utf8_lossy(&output.stderr).contains("tar header")
        );
    }
}

#[test]
fn invalid_identities_and_corrupt_cached_archives_fail() {
    let f = Fixture::new("preflight");
    let checksum = f.archive(&valid());
    f.lock(&checksum);
    let output = support::process::run(
        f.command()
            .args(["fetch", "--manifest-path"])
            .arg(f.0.join("Cargo.toml")),
        &f.0,
        "require-locked",
        Duration::from_secs(10),
    )
    .completed();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("fetch --locked"));
    let lock = std::fs::read_to_string(f.0.join("Cargo.lock")).unwrap();
    f.write("Cargo.lock",&(lock.clone()+"\n[[package]]\nname=\"other\"\nversion=\"1.0.0\"\nsource=\"registry+https://example.invalid/index\"\n"));
    let output = f.fetch("unsupported-source", &[]);
    assert!(!output.status.success());
    assert!(!f.0.join("cache").exists());
    f.write("Cargo.lock", &lock);
    success(&f.fetch("fetch", &[]), "seed cache");
    std::fs::remove_dir_all(
        f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0"),
    )
    .unwrap();
    f.write(
        "cache/registry/cache/index.crates.io-1949cf8c6b5b557f/sample-1.0.0.crate",
        "corrupt",
    );
    let output = f.fetch("corrupt-offline", &["--offline"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"));
    assert!(
        !f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0")
            .exists()
    );
    assert_eq!(
        std::fs::read_to_string(f.0.join("Cargo.lock")).unwrap(),
        lock
    );
}
#[test]
fn manifest_identity_and_gzip_errors_do_not_publish_sources() {
    for bad_gzip in [false, true] {
        let f = Fixture::new(if bad_gzip {
            "bad-gzip"
        } else {
            "wrong-manifest"
        });
        let mut tar = valid();
        if !bad_gzip {
            let at = tar
                .windows(b"version=\"1.0.0\"".len())
                .position(|s| s == b"version=\"1.0.0\"")
                .unwrap();
            tar[at + 9] = b'2';
        }
        let mut checksum = f.archive(&tar);
        if bad_gzip {
            let path = f.0.join("mirror/sample/1.0.0/download");
            let mut bytes = std::fs::read(&path).unwrap();
            bytes.pop();
            checksum = hash(&bytes);
            std::fs::write(path, bytes).unwrap();
        }
        f.lock(&checksum);
        let output = f.fetch("fetch", &[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(if bad_gzip {
                "invalid gzip"
            } else {
                "fetched manifest differs"
            }),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !f.0.join("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0")
                .exists()
        );
    }
}
