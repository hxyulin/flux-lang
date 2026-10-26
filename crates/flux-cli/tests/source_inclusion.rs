//! Physical includes and explicit compiler environment agree with rustc.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-source-inclusion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, path: &str, text: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn command(&self, compiler: &str, mode: &str, optimized: bool) -> Command {
        let mut cmd = if compiler == "rustc" {
            let mut cmd = Command::new("rustc");
            cmd.args(["--edition=2024", "-Cpanic=abort"]);
            if mode == "check" {
                cmd.args(["--emit=metadata", "-o"])
                    .arg(self.0.join("out.rmeta"));
            }
            cmd
        } else {
            let mut cmd = Command::new(env!("CARGO_BIN_EXE_flux"));
            cmd.arg(if mode == "check" { "mir" } else { "build" });
            cmd
        };
        cmd.env("FLUX_INCLUDE_ROOT", &self.0)
            .env("OUT_DIR", self.0.join("generated"))
            .env_remove("FLUX_INCLUDE_ABSENT")
            .current_dir(std::env::temp_dir())
            .arg(self.0.join("main.rs"));
        if mode == "build" {
            cmd.arg("-o").arg(self.exe(compiler, optimized));
            if optimized {
                cmd.arg("-O");
            }
        }
        cmd
    }
    fn exe(&self, compiler: &str, optimized: bool) -> PathBuf {
        self.0.join(format!("{compiler}-{optimized}"))
    }
    fn oracle(&self, accepts: bool, diagnostic_file: Option<&str>) {
        for compiler in ["rustc", "flux"] {
            let output = self.command(compiler, "check", false).output().unwrap();
            assert_eq!(
                output.status.success(),
                accepts,
                "{compiler}, source {}: {}",
                std::fs::read_to_string(self.0.join("main.rs")).unwrap(),
                String::from_utf8_lossy(&output.stderr)
            );
            if let Some(file) = diagnostic_file {
                let errors = String::from_utf8_lossy(&output.stderr);
                assert!(
                    errors.contains(&format!("{}:1:", self.0.join(file).display())),
                    "{compiler}: {errors}"
                );
            }
        }
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn runtime(&self) -> Vec<u8> {
        let mut reference = None;
        for optimized in [false, true] {
            for compiler in ["rustc", "flux"] {
                success(self.command(compiler, "build", optimized).output().unwrap());
                let result = success(
                    Command::new(self.exe(compiler, optimized))
                        .output()
                        .unwrap(),
                );
                assert!(result.stderr.is_empty());
                if let Some(expected) = &reference {
                    assert_eq!(&result.stdout, expected);
                } else {
                    reference = Some(result.stdout);
                }
            }
        }
        reference.unwrap()
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn nested_source_data_modules_hygiene_and_environment() {
    let case = Case::new();
    case.write("main.rs", r#"
include!(concat!(env!("FLUX_INCLUDE_ROOT"), "/physical/items.rs"));
include!(concat!(env!("OUT_DIR"), "/patterns.rs"));
#[cfg(any())] include!("absent.rs");
const INCLUDED: u32 = include!("physical/nested/value.rs");
static BYTES: &[u8; 4] = include_bytes!("physical/bytes");
#[track_caller]
fn check_location(file: &str, line: u32) {
    let caller = std::panic::Location::caller();
    assert_eq!(caller.file(), file);
    assert_eq!(caller.line(), line);
}
fn main() {
    assert_eq!(INCLUDED, 7);
    assert_eq!(BYTES[2], 128);
    assert_eq!(concat!("prefix:", include_str!("physical/value")), "prefix:value");
    let local = 11;
    let value = include!("physical/expression.rs");
    let bytes = include_bytes!(concat!("physical/", "bytes"));
    let empty = include_bytes!("physical/empty");
    macro_rules! wrapped { () => {{ let local = 17; include!("physical/expression.rs") }}; }
    assert_eq!(wrapped!(), 18);
    println!("{} {} {} {} {} {} {}", included(), value, bytes.len(), bytes[1], empty.len(), generated(), option_env!("FLUX_INCLUDE_ABSENT").is_none());
}
"#);
    case.write("physical/items.rs", r#"
mod child;
#[path = "nested/renamed.rs"] mod alias;
include!("nested/items.rs");
pub fn included() -> usize { assert_eq!(alias::value(), 6); child::value() + nested() + include_str!("value").len() }
"#);
    case.write("physical/child.rs", "pub fn value() -> usize { 5 }");
    case.write(
        "physical/nested/items.rs",
        r#"
macro_rules! data { () => { include_str!("data") }; }
pub fn nested() -> usize { data!().len() }
"#,
    );
    case.write("physical/nested/data", "nested");
    case.write(
        "physical/nested/renamed.rs",
        "pub fn value() -> usize { include_str!(\"data\").len() }",
    );
    case.write("physical/value", "value");
    case.write(
        "physical/expression.rs",
        "{ check_location(file!(), line!()); local + include!(\"nested/value.rs\") }",
    );
    case.write("physical/nested/value.rs", "7");
    case.write("physical/bytes", [0, 255, 128, 10]);
    case.write("physical/empty", "");
    case.write("generated/patterns.rs", "fn generated() -> usize { 29 }");
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"16 18 4 255 0 29 true\n");
}

#[test]
fn included_errors_use_physical_file_and_expansion_makes_progress() {
    let case = Case::new();
    for (source, included, diagnostic_file) in [
        (
            "include!(\"physical/items.rs\"); fn main() { broken(); }",
            "fn broken() { unknown(); }",
            "physical/items.rs",
        ),
        (
            "fn main() { let _: u32 = include!(\"physical/items.rs\"); }",
            "false",
            "physical/items.rs",
        ),
        (
            "include!(\"physical/items.rs\"); fn main() {}",
            "fn broken( {",
            "physical/items.rs",
        ),
        (
            "include!(\"physical/items.rs\"); fn main() {}",
            "include!(\"items.rs\");",
            "physical/items.rs",
        ),
    ] {
        case.write("main.rs", source);
        case.write("physical/items.rs", included);
        case.oracle(false, Some(diagnostic_file));
    }
    case.write("method.rs", "fn method() {}");
    for source in [
        "fn main() { include!(\"missing.rs\"); }",
        "fn main() { let _ = include_str!(env!(\"FLUX_INCLUDE_ABSENT\")); }",
        "fn main() { let _ = env!(\"FLUX_INCLUDE_ABSENT\", \"required explicit variable\"); }",
        "fn main() { let _ = include!(42); }",
        "macro_rules! recur { () => { recur!() }; } include!(recur!()); fn main() {}",
        "struct S; impl S { include!(\"method.rs\"); } fn main() {}",
    ] {
        case.write("main.rs", source);
        case.oracle(false, None);
    }
}

#[test]
fn eager_filename_macros_follow_textual_resolution() {
    let case = Case::new();
    case.write(
        "main.rs",
        r#"
macro_rules! filename { () => { concat!("physical/", "items.rs") }; }
include!(filename!());
fn main() { println!("{}", included()); }
"#,
    );
    case.write("physical/items.rs", "fn included() -> u32 { 31 }");
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"31\n");
}

fn keel_sources() -> PathBuf {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = std::fs::read_to_string(repo.join("Cargo.toml")).unwrap();
    let revision = manifest
        .lines()
        .find(|line| line.starts_with("keel-codegen = "))
        .unwrap()
        .split("rev = \"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let cargo = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".cargo"));
    for entry in std::fs::read_dir(cargo.join("git/checkouts")).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("keel-")
        {
            continue;
        }
        for checkout in std::fs::read_dir(path).unwrap() {
            let path = checkout.unwrap().path();
            if revision.starts_with(path.file_name().unwrap().to_str().unwrap())
                && path.join("crates/keel-codegen/src/select.rs").is_file()
            {
                return path;
            }
        }
    }
    panic!("the pinned Keel source checkout must exist after building flux");
}

#[test]
fn actual_keel_selector_include_and_generated_table() {
    let case = Case::new();
    let keel = keel_sources();
    let selector = std::fs::read_to_string(keel.join("crates/keel-codegen/src/select.rs")).unwrap();
    let inclusion = selector
        .lines()
        .find(|line| line.starts_with("include!(concat!(env!"))
        .unwrap();
    let build = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("build");
    let generated = std::fs::read_dir(build)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            path.file_name()?
                .to_str()?
                .starts_with("keel-codegen-")
                .then(|| path.join("out/patterns.rs"))
        })
        .find(|path| path.is_file())
        .expect("Keel generated selector source");
    let patterns = std::fs::read_to_string(&generated).unwrap();
    // The complete original selector source must reach its generated helper.
    // Other selector dependencies are outside this focused source-loading probe.
    let output = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("hir")
        .arg(keel.join("crates/keel-codegen/src/select.rs"))
        .env("OUT_DIR", generated.parent().unwrap())
        .output()
        .unwrap();
    let dump = String::from_utf8_lossy(&output.stdout);
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(dump.contains("fn generated_hint"), "{errors}\n{dump}");
    assert!(!errors.contains("cannot read"), "{errors}");

    // Execute the unchanged generated RULES declaration with its unchanged
    // production types. This is the table excerpt, not the full selector.
    let table = &patterns[..patterns.find("fn generated_hint").unwrap()];
    case.write("generated/patterns.rs", table);
    let declarations = &selector[selector
        .find("#[derive(Clone, Copy, PartialEq, Eq)]")
        .unwrap()..selector.find(inclusion).unwrap()];
    case.write("declarations.rs", declarations);
    let inst = std::fs::read_to_string(keel.join("crates/keel-ir/src/inst.rs")).unwrap();
    let start = inst.find("pub enum BinaryOp {").unwrap();
    let end = start + inst[start..].find("\n}").unwrap() + 2;
    let start = inst[..start].rfind("#[derive(").unwrap();
    case.write("binary_op.rs", &inst[start..end]);
    case.write("main.rs", format!(r#"
include!("binary_op.rs");
include!("declarations.rs");
{inclusion}
fn main() {{
    let mut digest = 0usize;
    for rule in RULES {{
        let target = match rule.target {{ Target::Aarch64 => 3, Target::X86 => 7 }};
        digest = digest * 31 + target + rule.root as usize + rule.child as usize + rule.commute as usize;
    }}
    println!("{{}} {{}}", RULES.len(), digest);
}}
"#));
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    case.runtime();
}

#[test]
fn impl_macro_items_retain_owner_and_match_rustc() {
    let case = Case::new();
    case.write(
        "main.rs",
        r#"
macro_rules! default_fn { ($($item:tt)*) => { $($item)* }; }
macro_rules! ty { () => { u32 }; }
macro_rules! constants { () => { const VALUE: u32 = 19; }; }
macro_rules! associated { () => { type Output = u32; }; }
struct Table(u32);
trait CloneTable { type Output; unsafe fn get(&self) -> Self::Output; }
impl Table {
    constants!();
    default_fn! { fn new(value: ty!()) -> Self { Self(value) } }
    default_fn! { fn method(&self) -> u32 { self.0 + 5 } }
}
impl CloneTable for Table {
    associated!();
    default_fn! { unsafe fn get(&self) -> Self::Output { self.0 + Self::VALUE } }
}
fn main() { let table = Table::new(23); println!("{} {}", unsafe { table.get() }, table.method()); }
"#,
    );
    case.write("method.rs", "fn method(&self) -> u32 { self.0 + 5 }");
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"42 28\n");
    for source in [
        "macro_rules! make { () => { fn duplicate() {} }; } struct S; impl S { fn duplicate() {} make!(); } fn main() {}",
        "macro_rules! make { () => { pub struct Nested; }; } struct S; impl S { make!(); } fn main() {}",
        "macro_rules! make { () => { make!(); }; } struct S; impl S { make!(); } fn main() {}",
    ] {
        case.write("main.rs", source);
        case.oracle(false, None);
    }
}

#[test]
fn body_macro_items_share_block_scopes_and_keep_item_boundaries() {
    let case = Case::new();
    case.write(
        "main.rs",
        r#"
macro_rules! items {
    ($name:ident, $value:expr) => {
        static $name: u32 = $value;
        const WIDTH: usize = 2;
        struct Pair([u32; WIDTH + 1]);
        impl Pair { fn value(&self) -> u32 { self.0[2] } }
        fn helper(arg: u32) -> u32 { arg + $name }
    };
}
macro_rules! lazy {
    ($name:ident, $value:expr) => {
        static $name: std::sync::LazyLock<u32> = std::sync::LazyLock::new(|| $value);
    };
}
fn main() {
    {
        println!("{}", helper(5));
        items!(FIRST, 19);
        let pair = Pair([2, 3, 7]);
        println!("{} {}", FIRST, pair.value());
        lazy!(LAZY, 31);
        println!("{}", *LAZY);
    }
    {
        items!(SECOND, 23);
        println!("{} {}", helper(7), SECOND);
    }
}
"#,
    );
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"24\n19 7\n31\n30 23\n");
    for source in [
        "macro_rules! make { () => { static LOCAL: u32 = 1; }; } fn main() { { make!(); } let _ = LOCAL; }",
        "macro_rules! make { () => { fn helper() {} }; } fn main() { { make!(); } helper(); }",
        "fn main() { let captured = 3; macro_rules! make { () => { fn helper() -> u32 { captured } }; } make!(); helper(); }",
        "fn outer<T>() { macro_rules! make { () => { fn helper(_: T) {} }; } make!(); } fn main() {}",
        "macro_rules! make { () => { static SAME: u32 = 1; }; } fn main() { make!(); make!(); }",
    ] {
        case.write("main.rs", source);
        case.oracle(false, None);
    }
}

#[test]
fn standard_macro_paths_and_split_type_fragments_match_rustc() {
    let case = Case::new();
    for declaration in [
        "std::thread_local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);}",
        "::std::thread_local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);}",
        "use std::thread_local as local; local!{static VALUE:std::cell::Cell<u32>=const{std::cell::Cell::new(1)};}",
        "use std as standard; standard::thread_local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);}",
        "local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);} use std::thread_local as local;",
    ] {
        case.write("main.rs", format!("{declaration} fn main() {{ VALUE.with(|v|{{v.set(v.get()+2);std::println!(\"{{}}\",v.get());}}); }}"));
        case.oracle(true, None);
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        assert_eq!(case.runtime(), b"3\n");
    }
    for declaration in [
        "use std::thread_local as local; local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);}",
        "std::thread_local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);}",
    ] {
        case.write("main.rs", format!("#![no_std]\nextern crate std;\n{declaration}\nfn main() {{ VALUE.with(|v|std::println!(\"{{}}\",v.get())); }}"));
        case.oracle(true, None);
    }
    case.write("main.rs", "fn main(){local!{static VALUE:std::cell::Cell<u32>=std::cell::Cell::new(1);} use std::thread_local as local; VALUE.with(|v|std::println!(\"{}\",v.get()));}");
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"1\n");
    case.write(
        "main.rs",
        r#"
macro_rules! typed { ($ty:ty = $value:expr) => {{ let value:$ty=$value; value.len() }}; }
macro_rules! trailing { ($ty:ty > $value:expr) => {{ let _:Option<$ty>=None; $value }}; }
macro_rules! token { ($value:tt) => { stringify!($value) }; }
fn main() {
    assert_eq!(typed!(Vec<Vec<u32>>=vec![vec![1,2]]),1);
    assert_eq!(trailing!(Vec<u32>>7),7);
    assert_eq!(token!(>>=), ">>=");
}
"#,
    );
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    case.runtime();
    for source in [
        "mod std {} std::thread_local!{static V:u32=1;} fn main(){}",
        "#![no_std] std::thread_local!{static V:u32=1;} fn main(){}",
        "mod hidden { use std::thread_local as private; } hidden::private!{static V:u32=1;} fn main(){}",
    ] {
        case.write("main.rs", source);
        case.oracle(false, None);
    }
}

#[test]
fn included_paths_in_macros_use_the_invocation_file() {
    let case = Case::new();
    case.write("data", "caller");
    case.write("value.rs", "11");
    case.write("defs/data", "definition");
    case.write("defs/value.rs", "23");
    case.write("defs/macros.rs","#[macro_export]macro_rules! text{()=>{include_str!(\"data\")};}#[macro_export]macro_rules! source{()=>{include!(\"value.rs\")};}");
    case.write(
        "main.rs",
        "#[path=\"defs/macros.rs\"]mod defs;fn main(){println!(\"{} {}\",text!(),source!());}",
    );
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert_eq!(case.runtime(), b"caller 11\n");
}

#[test]
fn shared_configuration_value_traits_match_rustc() {
    let case = Case::new();
    case.write(
        "main.rs",
        include_str!("../../../tests/run/arc_value_traits.rs"),
    );
    case.oracle(true, None);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    case.runtime();
}
