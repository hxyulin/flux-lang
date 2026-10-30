//! Rust 2015 source semantics, compared with the actual selected rustc edition.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

struct Case(PathBuf);
impl Case {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-edition-2015-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn check(
        &self,
        source: &str,
        edition: Edition,
        accepts: bool,
    ) -> Option<(flux_hir::Hir, flux_ty::TypedFile, flux_mir::FileMir)> {
        let path = self.write("main.rs", source);
        let output = Command::new("rustc")
            .arg(format!("--edition={}", edition_name(edition)))
            .args(["--crate-type=lib", "--emit=metadata", "--out-dir"])
            .arg(&self.0)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let root = db.load_with_edition(&path, edition).unwrap();
        let hir = flux_hir::lower(&mut db, root, Symbol::intern("main"));
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.iter().cloned());
        if !errors.is_empty() {
            assert!(!accepts, "{errors:?}");
            return None;
        }
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            assert!(!accepts, "{:?}", typed.diagnostics);
            return None;
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(mir.diagnostics.is_empty(), accepts, "{:?}", mir.diagnostics);
        accepts.then_some((hir, typed, mir))
    }
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    fn runtime(&self, source: &str, edition: Edition) -> Vec<u8> {
        let (hir, typed, mir) = self.check(source, edition, true).unwrap();
        let rustc = self.0.join("rustc");
        success(
            Command::new("rustc")
                .arg(format!("--edition={}", edition_name(edition)))
                .args(["-Cpanic=abort", "-Coverflow-checks=off"])
                .arg(self.0.join("main.rs"))
                .arg("-o")
                .arg(&rustc)
                .output()
                .unwrap(),
        );
        let expected = success(Command::new(rustc).output().unwrap());
        for optimized in [false, true] {
            compare_native(&self.0, &hir, &typed, &mir, &expected, optimized);
        }
        expected.stdout
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn edition_name(edition: Edition) -> &'static str {
    match edition {
        Edition::E2015 => "2015",
        Edition::E2018 => "2018",
        Edition::E2021 => "2021",
        Edition::E2024 => "2024",
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
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn compare_native(
    root: &Path,
    hir: &flux_hir::Hir,
    typed: &flux_ty::TypedFile,
    mir: &flux_mir::FileMir,
    expected: &Output,
    optimized: bool,
) {
    let tier = if optimized {
        flux_codegen::OptimizationTier::Full
    } else {
        flux_codegen::OptimizationTier::None
    };
    let mut module = flux_codegen::lower_module(hir, typed, mir).unwrap();
    let target = flux_codegen::Target::host().unwrap();
    let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
    let path = root.join(format!("flux-{optimized}"));
    flux_codegen::link_executable(&object, target, &path).unwrap();
    let actual = Command::new(path).output().unwrap();
    assert_eq!(actual.status.code(), expected.status.code());
    assert_eq!(
        actual.stdout,
        expected.stdout,
        "{}",
        std::fs::read_to_string(root.join("main.rs")).unwrap()
    );
    assert_eq!(actual.stderr, expected.stderr);
}

#[test]
fn identifiers_and_contextual_dyn_match_rustc() {
    let case = Case::new();
    let names = r#"struct dyn(u32);fn async()->u32{1}fn await()->u32{2}fn try()->u32{3}fn main(){let value=dyn(4);println!("{}",async()+await()+try()+value.0);}"#;
    case.check(names, Edition::E2015, true);
    case.check(names, Edition::E2018, false);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(case.runtime(names, Edition::E2015), b"10\n");
    let object = "trait Value{fn value(&self)->u32;}impl Value for u32{fn value(&self)->u32{*self}}fn read(value:&dyn Value)->u32{value.value()}fn main(){println!(\"{}\",read(&7));}";
    case.check(object, Edition::E2015, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(case.runtime(object, Edition::E2015), b"7\n");
    let path = "mod dyn{pub struct Value;}fn take(_:dyn::Value){}fn main(){take(dyn::Value);}";
    case.check(path, Edition::E2015, true);
    case.check(path, Edition::E2018, false);
}

#[test]
fn bare_traits_and_root_imports_are_edition_sensitive() {
    let case = Case::new();
    let bare = "trait Value{fn value(&self)->u32;}impl Value for u32{fn value(&self)->u32{*self}}fn id<'a>(value:&'a Value)->&'a Value{value}fn main(){println!(\"{}\",id(&9).value());}";
    for edition in [Edition::E2015, Edition::E2018] {
        case.check(bare, edition, true);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(case.runtime(bare, edition), b"9\n");
    }
    case.check(bare, Edition::E2021, false);
    let rooted = "mod source{pub fn value()->u32{11}}mod inside{mod source{pub fn value()->u32{22}}use source::value;pub fn run()->u32{value()}}fn main(){println!(\"{}\",inside::run());}";
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        assert_eq!(case.runtime(rooted, Edition::E2015), b"11\n");
        assert_eq!(case.runtime(rooted, Edition::E2018), b"22\n");
    }
    let global = "mod local{pub fn value()->u32{13}}fn main(){println!(\"{}\",::local::value());}";
    case.check(global, Edition::E2015, true);
    case.check(global, Edition::E2018, false);
    case.check("use core::cmp::Ordering;fn main(){}", Edition::E2015, false);
    case.check(
        "#![no_std]use core::cmp::Ordering;fn f()->Ordering{Ordering::Equal}",
        Edition::E2015,
        true,
    );
    case.check(
        "mod m{pub mod n{pub(in m)struct Value;}}fn main(){}",
        Edition::E2015,
        true,
    );
    case.check(
        "mod m{pub mod n{pub(in m)struct Value;}}fn main(){}",
        Edition::E2018,
        false,
    );
}

#[test]
fn shared_legacy_semantics_and_macro_imports() {
    let case = Case::new();
    let source = r#"macro_rules! route{($e:expr)=>{1};(_)=>{2};(const $e:expr)=>{3};}macro_rules! pats{($($p:pat)|+)=>{{let mut n=0;$(let _=stringify!($p);n+=1;)+n}};}fn main(){let values=[String::from("first"),String::from("second")];let mut iter=values.into_iter();let first:&String=iter.next().unwrap();println!("{} {} {} {} {}",first,values[1],route!(_),route!(const{3}),pats!(A|B));if false{panic!("literal {missing}");}}"#;
    case.check(source, Edition::E2015, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(
        case.runtime(source, Edition::E2015),
        b"first second 2 3 2\n"
    );
    let capture = "fn main(){let pair=(String::from(\"one\"),String::from(\"two\"));let f=||pair.0.len();drop(pair.1);println!(\"{}\",f());}";
    case.check(capture, Edition::E2015, false);
    case.check(capture, Edition::E2021, true);
    let macro_import =
        "macro_rules!m{()=>{7};}use m as renamed;fn main(){println!(\"{}\",renamed!());}";
    case.check(macro_import, Edition::E2015, false);
    case.check(macro_import, Edition::E2018, true);
    case.check(
        "fn main(){let _:u8=u8::try_from(3u32).unwrap();}",
        Edition::E2015,
        false,
    );
    case.check(
        "#![no_std]#[macro_use]extern crate std;fn f(){println!(\"standard macro\");}",
        Edition::E2015,
        true,
    );
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        let source = include_str!("fixtures/edition2021/if_let_drop_order.rs");
        assert_eq!(
            case.runtime(source, Edition::E2015),
            case.runtime(source, Edition::E2018)
        );
    }
}

#[test]
fn external_crates_and_mixed_edition_macro_tokens() {
    let case = Case::new();
    let library=case.write("dep.rs",r#"pub fn value()->u32{17}#[macro_export]macro_rules!declare{()=>{pub fn async()->u32{23}};}#[macro_export]macro_rules!seven{()=>{7u32};}#[macro_export]macro_rules!bare{()=>{pub fn read(value:&Trait)->u32{value.value()}};}#[macro_export]macro_rules!rooted{()=>{pub fn root_value()->u32{::local::value()}};}"#);
    let rlib = case.0.join("libdep.rlib");
    success(
        Command::new("rustc")
            .args([
                "--edition=2015",
                "--crate-name=dep",
                "--crate-type=rlib",
                "-Cpanic=abort",
            ])
            .arg(&library)
            .arg("-o")
            .arg(&rlib)
            .output()
            .unwrap(),
    );
    for (edition, source, accepts, expected) in [
        (
            Edition::E2015,
            "use dep::value;fn main(){println!(\"{}\",value());}",
            false,
            "",
        ),
        (
            Edition::E2018,
            "use dep::value;fn main(){println!(\"{}\",value());}",
            true,
            "17\n",
        ),
        (
            Edition::E2015,
            "extern crate dep as named;use named::value;fn main(){println!(\"{}\",value());}",
            true,
            "17\n",
        ),
        (
            Edition::E2015,
            "fn main(){println!(\"{}\",dep::value());}",
            true,
            "17\n",
        ),
        (
            Edition::E2015,
            "fn main(){println!(\"{}\",::dep::value());}",
            false,
            "",
        ),
        (
            Edition::E2015,
            "extern crate dep;fn main(){println!(\"{}\",::dep::value());}",
            true,
            "17\n",
        ),
        (
            Edition::E2015,
            "#[macro_use]extern crate dep;mod inner{pub fn value()->u32{seven!()}}fn main(){println!(\"{}\",inner::value());}",
            true,
            "7\n",
        ),
        (
            Edition::E2024,
            "dep::declare!();fn main(){println!(\"{}\",r#async());}",
            true,
            "23\n",
        ),
        (
            Edition::E2024,
            "trait Trait{fn value(&self)->u32;}impl Trait for u32{fn value(&self)->u32{*self}}dep::bare!();fn main(){println!(\"{}\",read(&29));}",
            true,
            "29\n",
        ),
        (
            Edition::E2024,
            "mod local{pub fn value()->u32{31}}dep::rooted!();fn main(){println!(\"{}\",root_value());}",
            true,
            "31\n",
        ),
    ] {
        let path = case.write("main.rs", source);
        let reference = case.0.join("reference");
        let output = Command::new("rustc")
            .arg(format!("--edition={}", edition_name(edition)))
            .args(["--extern"])
            .arg(format!("dep={}", rlib.display()))
            .args(["-Cpanic=abort", "-o"])
            .arg(&reference)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let lib = db.load_with_edition(&library, Edition::E2015).unwrap();
        let main = db.load_with_edition(&path, edition).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: lib,
                    name: Symbol::intern("dep"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: main,
                    name: Symbol::intern("main"),
                    dependencies: vec![(Symbol::intern("dep"), 0)],
                },
            ],
        );
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics.iter().cloned());
        if !diagnostics.is_empty() {
            assert!(!accepts, "{source}: {diagnostics:?}");
            continue;
        }
        let typed = flux_mir::check_file(&hir);
        if !typed.diagnostics.is_empty() {
            assert!(!accepts, "{source}: {:?}", typed.diagnostics);
            continue;
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepts,
            "{source}: {:?}",
            mir.diagnostics
        );
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        if accepts {
            let output = success(Command::new(&reference).output().unwrap());
            assert_eq!(output.stdout, expected.as_bytes());
            for optimized in [false, true] {
                compare_native(&case.0, &hir, &typed, &mir, &output, optimized);
            }
        }
    }
}
