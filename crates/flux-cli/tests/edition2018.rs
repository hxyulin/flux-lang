//! Rust 2018 source semantics, compared directly with rustc's edition 2018.
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
            "flux-edition-2018-{}-{}",
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
            .args(["--emit=metadata", "--out-dir"])
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
fn array_method_compatibility_and_owned_ufcs() {
    let case = Case::new();
    let borrowed = r#"fn main(){let a=[String::from("a"),String::from("b")];let mut iter=a.into_iter();let first:&String=iter.next().unwrap();println!("{} {}",first,a[1]);}"#;
    case.check(borrowed, Edition::E2018, true);
    case.check(borrowed, Edition::E2024, false);
    case.check("fn main(){let mut it=[String::from(\"short\")].into_iter();println!(\"{}\",it.next().unwrap());}",Edition::E2018,false);
    let custom = "trait Other{fn into_iter(self)->u32;}impl Other for [u32;2]{fn into_iter(self)->u32{self[0]+self[1]}}fn main(){println!(\"{}\",[3,4].into_iter());}";
    case.check(custom, Edition::E2018, true);
    case.check(custom, Edition::E2024, false);
    let owned = r#"fn first<I:IntoIterator>(v:I)->I::Item{v.into_iter().next().unwrap()}fn main(){let s=first([String::from("owned")]);let mut i=IntoIterator::into_iter([String::from("ufcs")]);let x:String=i.next().unwrap();println!("{} {}",s,x);for s in [String::from("for")] {println!("{}",s);}}"#;
    case.check(owned, Edition::E2018, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        assert_eq!(case.runtime(borrowed, Edition::E2018), b"a b\n");
        assert_eq!(case.runtime(owned, Edition::E2018), b"owned ufcs\nfor\n");
        assert_eq!(case.runtime(custom, Edition::E2018), b"7\n");
    }
}

#[test]
fn macro_fragments_and_legacy_literal_panic() {
    let case = Case::new();
    let legacy = r#"
macro_rules! patterns {($($p:pat)|+) => {{let mut count=0;$(let _=stringify!($p);count+=1;)+count}};}
macro_rules! route {($e:expr)=>{1};(_)=>{2};(const $e:expr)=>{3};}
fn gen()->u32{17}
fn main(){println!("{} {} {} {}",patterns!(A|B),route!(_),route!(const {4}),gen());if false {panic!("literal {missing}");}}
"#;
    case.check(legacy, Edition::E2018, true);
    case.check(legacy, Edition::E2024, false);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(case.runtime(legacy, Edition::E2018), b"2 2 3 17\n");
}

#[test]
fn old_prelude_and_match_ergonomics() {
    let case = Case::new();
    for source in [
        "fn main(){let _:u8=u8::try_from(5u16).unwrap();}",
        "fn main(){let _:u8=5u16.try_into().unwrap();}",
        "fn main(){let _:Vec<u32>=Vec::from_iter([1,2]);}",
    ] {
        case.check(source, Edition::E2018, false);
        case.check(source, Edition::E2024, true);
    }
    let source = r#"use std::convert::TryInto;use std::iter::FromIterator;static mut VALUE:u32=3;fn main(){let [ref x]=&[5u32];let [mut y]=&[7u32];y+=1;let z:u8=9u16.try_into().unwrap();let v=Vec::from_iter([1u32,2]);println!("{} {} {} {} {}",x,y,z,v.len(),unsafe{*(&VALUE)});}"#;
    case.check(source, Edition::E2018, true);
    case.check(source, Edition::E2024, false);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(case.runtime(source, Edition::E2018), b"5 8 9 2 3\n");
}

#[test]
fn closure_capture_and_temporary_drop_scopes_differ() {
    let case = Case::new();
    let capture = r#"fn main(){let pair=(String::from("first"),String::from("second"));let f=||pair.0.len();drop(pair.1);println!("{}",f());}"#;
    case.check(capture, Edition::E2018, false);
    case.check(capture, Edition::E2024, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    {
        for (index,source) in [
            include_str!("fixtures/edition2021/drop_order.rs"),
            include_str!("fixtures/edition2021/if_let_drop_order.rs"),
            "struct D(u32);impl Drop for D{fn drop(&mut self){println!(\"drop {}\",self.0);}}fn main(){let pair=(D(1),D(2));let f=move||pair.0.0;drop(f);println!(\"after\");}",
        ].into_iter().enumerate() {let old=case.runtime(source,Edition::E2018);let new=case.runtime(source,Edition::E2024);let mid=case.runtime(source,Edition::E2021);assert_ne!(old,new);assert_eq!(mid,if index<2{old}else{new});}
    }
}

#[test]
fn macro_token_editions_cross_crate_boundaries() {
    for (library_edition, consumer_edition, library_source, consumer_source, expected) in [
        (
            Edition::E2018,
            Edition::E2024,
            r#"
#[macro_export]macro_rules! iterate{($a:expr)=>{$a.into_iter()};}
#[macro_export]macro_rules! route{($e:expr)=>{1};(const $e:expr)=>{3};}
#[macro_export]macro_rules! declare{()=>{fn gen()->u32{17}};}
"#,
            r#"source::declare!();fn main(){let a=[1u32,2];let mut it=source::iterate!(a);let x:&u32=it.next().unwrap();println!("{} {} {}",x,source::route!(const{4}),r#gen());}"#,
            "1 3 17\n",
        ),
        (
            Edition::E2024,
            Edition::E2018,
            r#"
#[macro_export]macro_rules! iterate{($a:expr)=>{$a.into_iter()};}
#[macro_export]macro_rules! route{($e:expr)=>{1};(const $e:expr)=>{3};}
#[macro_export]macro_rules! declare{($name:ident)=>{fn $name()->u32{23}};}
"#,
            r#"source::declare!(gen);fn main(){let a=[1u32,2];let mut it=source::iterate!(a);let x:u32=it.next().unwrap();println!("{} {} {}",x,source::route!(const{4}),gen());}"#,
            "1 1 23\n",
        ),
    ] {
        let case = Case::new();
        let library = case.write("source.rs", library_source);
        let consumer = case.write("main.rs", consumer_source);
        let lib = case.0.join("libsource.rlib");
        success(
            Command::new("rustc")
                .arg(format!("--edition={}", edition_name(library_edition)))
                .args(["--crate-type=rlib", "--crate-name=source", "-Cpanic=abort"])
                .arg(&library)
                .arg("-o")
                .arg(&lib)
                .output()
                .unwrap(),
        );
        let executable = case.0.join("rustc");
        success(
            Command::new("rustc")
                .arg(format!("--edition={}", edition_name(consumer_edition)))
                .arg("-Cpanic=abort")
                .arg(&consumer)
                .arg("--extern")
                .arg(format!("source={}", lib.display()))
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap(),
        );
        let mut db = SourceDb::new();
        let lib = db.load_with_edition(&library, library_edition).unwrap();
        let main = db.load_with_edition(&consumer, consumer_edition).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: lib,
                    name: Symbol::intern("source"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: main,
                    name: Symbol::intern("main"),
                    dependencies: vec![(Symbol::intern("source"), 0)],
                },
            ],
        );
        assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        {
            let output = success(Command::new(executable).output().unwrap());
            assert_eq!(output.stdout, expected.as_bytes());
            for optimized in [false, true] {
                compare_native(&case.0, &hir, &typed, &mir, &output, optimized);
            }
        }
    }
}

#[test]
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn legacy_temporaries_survive_enclosing_expression_and_early_exit() {
    let case = Case::new();
    for source in [
        "struct D(u32);impl Drop for D{fn drop(&mut self){println!(\"drop {}\",self.0);}}fn combine(_:u32,_:u32){}fn main(){combine({let _local=D(1);D(2).0},{println!(\"arg\");0});}",
        "struct D(u32);impl Drop for D{fn drop(&mut self){println!(\"drop {}\",self.0);}}fn value()->u32{let _local=D(1);{let _inner=D(2);return D(3).0;}}fn main(){println!(\"value {}\",value());}",
        "struct D(u32);impl Drop for D{fn drop(&mut self){println!(\"drop {}\",self.0);}}fn value()->u32{if let (Some(_),_)=(None::<u32>,D(1)){0}else{let _local=D(2);D(3).0}}fn main(){println!(\"value {}\",value());}",
    ] {
        case.runtime(source, Edition::E2018);
        case.runtime(source, Edition::E2021);
        case.runtime(source, Edition::E2024);
    }
}

#[test]
fn old_edition_rejects_2024_let_chains() {
    let case = Case::new();
    let source = "fn main(){if let Some(x)=Some(2) && x>0 {println!(\"{}\",x);}}";
    case.check(source, Edition::E2018, false);
    case.check(source, Edition::E2024, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    case.runtime(
        "fn take<T>(_:T){}fn main(){if false{take(loop{});}println!(\"done\");}",
        Edition::E2018,
    );
}

#[test]
fn argument_impl_trait_uses_original_legacy_editions() {
    let case = Case::new();
    let source = r#"
fn apply(mut f: impl FnMut(&mut u32) -> bool, mut value: u32) -> u32 {
    if f(&mut value) { value } else { 0 }
}

fn inspect(f: impl Fn(u32) -> bool, value: u32) -> bool { f(value) }
fn main() {
    let extra = 5;
    let n = apply(|x| { *x += extra; true }, 7);
    println!("{} {}", n, inspect(|x| x == n, 12));
}
"#;
    for edition in [Edition::E2018, Edition::E2021, Edition::E2024] {
        case.check(source, edition, true);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(case.runtime(source, edition), b"12 true\n");
    }
}

#[test]
fn boxed_slice_into_iter_respects_edition_2024_boundary() {
    let case = Case::new();
    let borrowed = r#"fn main(){let values:Box<[String]>=vec![String::from("first"),String::from("second")].into_boxed_slice();let mut iter=values.into_iter();let first:&String=iter.next().unwrap();println!("{} {}",first,values[1]);}"#;
    for edition in [Edition::E2018, Edition::E2021] {
        case.check(borrowed, edition, true);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(case.runtime(borrowed, edition), b"first second\n");
    }
    case.check(borrowed, Edition::E2024, false);
    let owned = r#"fn main(){let values:Box<[String]>=vec![String::from("owned")].into_boxed_slice();let mut iter=IntoIterator::into_iter(values);let value:String=iter.next().unwrap();println!("{}",value);for value in vec![String::from("loop")].into_boxed_slice(){let value:String=value;println!("{}",value);}}"#;
    for edition in [Edition::E2018, Edition::E2021, Edition::E2024] {
        case.check(owned, edition, true);
        #[cfg(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        ))]
        assert_eq!(case.runtime(owned, edition), b"owned\nloop\n");
    }
    let modern = r#"fn main(){let values:Box<[String]>=vec![String::from("modern")].into_boxed_slice();let value:String=values.into_iter().next().unwrap();println!("{}",value);}"#;
    case.check(modern, Edition::E2024, true);
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    assert_eq!(case.runtime(modern, Edition::E2024), b"modern\n");
}
