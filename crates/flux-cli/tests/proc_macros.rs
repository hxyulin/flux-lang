//! Real host macros with native rustc/Flux output comparisons.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(90)).completed()
}
fn write(root: &Path, path: &str, text: &str) {
    let p = root.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}
const MACROS: &str = r##"
extern crate proc_macro;
use proc_macro::{TokenStream,TokenTree};
static COUNT:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
#[proc_macro] pub fn counter(_:TokenStream)->TokenStream {COUNT.fetch_add(1,std::sync::atomic::Ordering::SeqCst).to_string().parse().unwrap()}
#[proc_macro] pub fn identity(input:TokenStream)->TokenStream {input}
#[proc_macro] pub fn suffix(input:TokenStream)->TokenStream {
    let mut input=input.into_iter();let literal=input.next().unwrap();
    assert!(matches!(literal,TokenTree::Literal(_)));assert!(input.next().is_none());
    assert_eq!(literal.to_string(),"\"word\"custom");"true".parse().unwrap()
}
#[proc_macro] pub fn separate(input:TokenStream)->TokenStream {assert_eq!(input.into_iter().count(),2);"true".parse().unwrap()}
#[proc_macro] pub fn emit_suffix(_:TokenStream)->TokenStream {"host::suffix!(\"word\"custom)".parse().unwrap()}
#[proc_macro_attribute] pub fn docs(_:TokenStream,input:TokenStream)->TokenStream {assert!(input.to_string().contains("doc"));assert!(input.to_string().contains("retained"));input}
#[proc_macro] pub fn host_cwd(_:TokenStream)->TokenStream {format!("{:?}",std::env::current_dir().unwrap().to_str().unwrap()).parse().unwrap()}
#[proc_macro] pub fn environment(_:TokenStream)->TokenStream {format!("{:?}",std::env::var("MACRO_VALUE").unwrap()).parse().unwrap()}
#[proc_macro] pub fn inspect(input:TokenStream)->TokenStream {
    let t=input.into_iter().next().unwrap();let span=t.span();
    let text=span.source_text().unwrap();let length=text.len();
    format!("({:?},{}usize,{}usize,{}usize)",text,span.line(),span.column(),length).parse().unwrap()
}
#[proc_macro] pub fn mixed_capture(_:TokenStream)->TokenStream {
    TokenTree::Ident(proc_macro::Ident::new("outside",proc_macro::Span::mixed_site())).into()
}
#[proc_macro] pub fn call_capture(_:TokenStream)->TokenStream {"outside".parse().unwrap()}
#[proc_macro_attribute] pub fn remove(_:TokenStream,_:TokenStream)->TokenStream{TokenStream::new()}
#[proc_macro] pub fn failure(_:TokenStream)->TokenStream {"compile_error!(\"macro rejected input\");".parse().unwrap()}
#[proc_macro] pub fn panics(_:TokenStream)->TokenStream {panic!("deliberate macro panic")}
#[proc_macro_attribute] pub fn add(arguments:TokenStream,input:TokenStream)->TokenStream {
    let mut output=input;output.extend(format!("fn added()->usize{{ {arguments} }}").parse::<TokenStream>().unwrap());output
}
#[proc_macro_derive(Answer,attributes(answer))] pub fn answer(input:TokenStream)->TokenStream {
    assert!(input.to_string().contains("answer"),"helper input lost");
    assert!(!input.to_string().contains("missing"),"disabled field leaked to derive");
    let mut iter=input.into_iter();let mut name=None;
    while let Some(t)=iter.next(){if matches!(&t,TokenTree::Ident(i) if i.to_string()=="struct"){name=iter.next().map(|t|t.to_string());break;}}
    format!("impl {}{{fn answer()->usize{{42}}}}",name.unwrap()).parse().unwrap()
}

macro_rules! declare_generated {
    ($bang:ident, $attribute:ident, $derive:ident, $helper:ident) => {
        #[proc_macro] pub fn $bang(input:TokenStream)->TokenStream {input}
        #[proc_macro_attribute] pub fn $attribute(_:TokenStream,input:TokenStream)->TokenStream {input}
        #[proc_macro_derive($derive)] pub fn generated_derive(input:TokenStream)->TokenStream {
            let mut iter=input.into_iter();let mut name=None;
            while let Some(t)=iter.next(){if matches!(&t,TokenTree::Ident(i) if i.to_string()=="struct"){name=iter.next().map(|t|t.to_string());break;}}
            format!("impl {}{{fn generated()->usize{{43}}}}",name.unwrap()).parse().unwrap()
        }
        #[proc_macro_derive(GeneratedHelper,attributes($helper))]
        pub fn generated_helper(_:TokenStream)->TokenStream {TokenStream::new()}
    };
}
declare_generated!(generated_identity, generated_attribute, Generated, generated_helper_attr);
#[cfg(any())] declare_generated!(disabled_identity, disabled_attribute, Disabled, disabled_helper);
"##;
const PROGRAM: &str = r####"
use host::{identity as id,add,Answer};
#[derive(Clone,Answer)] #[answer(rename="kept")]
/// retained root docs
struct Item { #[answer] value:usize, #[cfg(any())] missing:MissingType }
#[add(29)] fn original()->usize{13}
id!(fn generated()->usize{17});
id!(/// retained item docs
    struct Documented;);
#[host::docs]
/// retained function docs
fn documented()->usize{23}
#[derive(host::Generated)] struct GeneratedItem;
#[::core::prelude::v1::derive(::core::clone::Clone, ::core::marker::Copy, ::core::fmt::Debug)]
struct Qualified(u32);
#[host::generated_attribute] fn generated_attribute_value()->usize{host::generated_identity!(31)}
type Number=id!(usize);
fn borrowed<'a>(x:&'a Number)->&'a Number{id!(x)}
host::identity! { fn anonymous(_: &'_ Number, value: Number) -> Number { value } }
macro_rules! forwarded_local { ($value:expr) => {{ let generated = $value; host::identity!(generated) }}; }
mod later_consumer {
    use super::late_prelude::*;
    identity! { pub struct Late; }
    #[crate::late_prelude::docs]
    #[doc="retained"]
    pub fn attributed() -> usize { 47 }
    #[crate::late_prelude::remove]
    fn erased(_: MissingType) {}
    impl Late { identity! { pub fn value() -> usize { 29 } } }
}
mod late_prelude { pub use host::{identity, docs, remove}; }
fn main(){
    let qualified=Qualified(37); assert_eq!(qualified.clone().0,37);
    assert_eq!(forwarded_local!(17),17);
    assert_eq!(anonymous(&3,7),7);
    assert_eq!(later_consumer::Late::value(),29);
    assert_eq!(later_consumer::attributed(),47);
    assert_eq!(GeneratedItem::generated(),43);assert_eq!(generated_attribute_value(),31);
    let a=host::counter!();let b=host::counter!();assert_ne!(a,b);assert_eq!(a+b,1);
    let value=Item{value:3};assert_eq!(value.clone().value,3);assert_eq!(Item::answer(),42);
    assert_eq!(original()+added()+generated(),59);
    assert_eq!(documented(),23);let _=Documented;
    assert!(host::suffix!("word"custom));assert!(host::emit_suffix!());
    macro_rules! separated {($l:literal,$i:ident)=>{host::separate!($l $i)}}
    assert!(separated!("word",custom));
    let n=7;assert_eq!(*borrowed(&n),7);
    assert_eq!(id!((1u32 << 3) >= 8),true);
    assert_eq!(id!(r###"raw " value"###),"raw \" value");
    assert_eq!(id!(b'X'),88);assert_eq!(id!(b"bytes"),b"bytes");
    assert_eq!(id!(1.25e1f64),12.5);assert_eq!(id!(0xff_u32),255);
    assert_eq!(host::environment!(),"target environment");
    assert_eq!(std::path::Path::new(host::host_cwd!()),std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap());
    let café=0;let span=host::inspect!(n);assert_eq!(span.0,"n");assert_eq!(span.3,1);let _=café;
    let group=host::inspect!((n +
        1));assert_eq!(group.0,"(n +\n        1)");assert_eq!(group.3,group.0.len());
    println!("macros {} {} {} {} {}",Item::answer(),added(),span.0,span.1,span.2);
}
"####;
#[test]
fn native_macros_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/proc-macros/{}", std::process::id()));
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=[\"app\",\"host\"]\nresolver=\"3\"\n",
    );
    write(
        &root,
        "host/Cargo.toml",
        "[package]\nname=\"host\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\nproc-macro=true\n",
    );
    write(&root, "host/src/lib.rs", MACROS);
    write(
        &root,
        "app/Cargo.toml",
        "[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nhost={path=\"../host\"}\n",
    );
    write(
        &root,
        "app/build.rs",
        "fn main(){println!(\"cargo::rustc-env=MACRO_VALUE=target environment\");}",
    );
    write(&root, "app/src/main.rs", PROGRAM);
    let manifest = root.join("Cargo.toml");
    support::oracle::success(
        &run(
            Command::new("cargo")
                .args(["generate-lockfile", "--offline", "--manifest-path"])
                .arg(&manifest),
            &root,
            "lock",
        ),
        "lock",
    );
    let expected = run(
        Command::new("cargo")
            .args(["run", "--quiet", "--offline", "--locked", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app"]),
        &root,
        "rustc",
    );
    support::oracle::success(&expected, "Rust macro reference");
    for optimize in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .args(["cargo", "build", "--offline", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app"]);
        if optimize {
            command.arg("-O");
        }
        let output = run(&mut command, &root, &format!("flux-build-{optimize}"));
        support::oracle::success(&output, "Flux macros build");
        let output = run(
            &mut Command::new(root.join("target/flux/app")),
            &root,
            &format!("flux-run-{optimize}"),
        );
        support::oracle::success(&output, "Flux macro native");
        assert_eq!(output.stdout, expected.stdout, "optimized={optimize}");
        assert_eq!(output.stderr, expected.stderr);
    }
}

#[test]
fn direct_expansion_contracts_match_rustc() {
    use flux_hir::{CrateCfg, SourceCrate, SourceDb};
    use flux_intern::Symbol;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/proc-macro-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("host.rs");
    std::fs::write(&source, MACROS).unwrap();
    let dylib = root.join("libhost.dylib");
    let output = run(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=proc-macro",
                "--crate-name=host",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&dylib),
        &root,
        "host-build",
    );
    support::oracle::success(&output, "host dylib");
    let version = Command::new("rustc").arg("-vV").output().unwrap();
    let mut cfg = CrateCfg::default();
    cfg.names.push(Symbol::intern("proc_macro"));
    let exports = flux_cli::proc_macro::exports(
        &String::from_utf8(version.stdout).unwrap(),
        &dylib,
        &source,
        cfg,
    )
    .unwrap();
    assert!(
        exports
            .iter()
            .find(|m| m.name.as_str() == "Generated")
            .unwrap()
            .helpers
            .is_none()
    );
    assert!(
        exports
            .iter()
            .find(|m| m.name.as_str() == "GeneratedHelper")
            .unwrap()
            .helpers
            .is_none()
    );
    assert!(!exports.iter().any(|m| m.name.as_str() == "Disabled"));
    assert!(
        exports
            .iter()
            .find(|m| m.name.as_str() == "Answer")
            .unwrap()
            .helpers
            .as_ref()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == "answer")
    );
    for (name, text, expected, flux_expected) in [
        ("generated_function_helper_shadow", "use host::{GeneratedHelper,remove as generated_helper_attr};#[generated_helper_attr]#[derive(GeneratedHelper)]fn bad(){}fn main(){}", false),
        ("generated_derive_before_attribute", "#[derive(host::GeneratedHelper)]#[host::remove]fn bad(){}fn main(){}", false),
        ("generated_attribute_before_derive", "#[host::remove]#[derive(host::GeneratedHelper)]fn bad(){}fn main(){}", true),
        ("generated_disabled_variant", "#[derive(host::GeneratedHelper)]enum E{#[cfg(any())]Disabled{#[allow(dead_code)]value:Missing},Enabled}fn main(){}", true),
        ("generated_qualified_attribute", "#[derive(host::Generated)]#[host::generated_attribute]struct S;fn main(){assert_eq!(S::generated(),43);}", true),
        ("generated_configured_fields", "#[cfg_attr(all(),derive(host::Generated))]struct S{#[cfg(any())]#[allow(dead_code)]missing:Missing,n:u32}fn main(){assert_eq!(S::generated(),43);}", true),
        ("generated_helper_macro_shadow", "use host::{GeneratedHelper, add as generated_helper_attr};#[derive(GeneratedHelper)]#[generated_helper_attr(37)]struct S;fn main(){let _=added();}", false),
        ("generated_derive", "#[derive(host::Generated)]struct S;fn main(){assert_eq!(S::generated(),43);}", true),
        ("generated_attribute", "#[host::generated_attribute]fn f(){}fn main(){f();}", true),
        ("generated_function", "host::generated_identity!(fn f(){});fn main(){f();}", true),
        ("generated_unknown_helper", "#[derive(host::Generated)]#[invented_helper]struct S;fn main(){}", false),
        ("disabled_generated_export", "#[derive(host::Disabled)]struct S;fn main(){}", false),
        (
            "qualified",
            "host::identity!(fn value()->u32{7});fn main(){assert_eq!(value(),7);}",
            true,
        ),
        (
            "alias",
            "use host::Answer as Build;#[derive(Build)]#[answer]struct S;fn main(){assert_eq!(S::answer(),42);}",
            true,
        ),
        (
            "helper_field",
            "#[derive(host::Answer)]struct S{#[answer]n:u32}fn main(){}",
            true,
        ),
        (
            "configured_derive",
            "#[cfg_attr(all(),derive(host::Answer),answer)]struct S;fn main(){assert_eq!(S::answer(),42);}",
            true,
        ),
        (
            "attribute_import_removed",
            "#[host::remove]use missing::Type;fn main(){}",
            true,
        ),
        (
            "removed_import_unavailable",
            "#[host::remove]use core::mem::replace;fn main(){let mut x=0;let _=replace(&mut x,1);}",
            false,
        ),
        (
            "unknown_helper",
            "#[derive(host::Answer)]#[answer]#[unknown_helper]struct S;fn main(){}",
            false,
        ),
        (
            "helper_does_not_escape",
            "#[derive(host::Answer)]#[answer]struct S;#[answer]struct Other;fn main(){}",
            false,
        ),
        (
            "malformed_derive",
            "#[derive(,host::Answer)]#[answer]struct S;fn main(){}",
            false,
        ),
        (
            "malformed_second_derive",
            "#[derive]#[derive(host::Answer)]#[answer]struct S;fn main(){}",
            false,
        ),
        (
            "wrong_derive_kind",
            "#[derive(host::identity)]struct S;fn main(){}",
            false,
        ),
        (
            "attribute_eq",
            "#[host::add=7]fn value(){}fn main(){}",
            false,
        ),
        ("missing_macro", "fn main(){host::missing!();}", false),
        ("panic", "fn main(){host::panics!();}", false),
        ("compile_error", "host::failure!();fn main(){}", false),
        (
            "mixed_capture",
            "fn main(){let outside=7;let _=host::mixed_capture!();}",
            false,
        ),
        (
            "call_capture",
            "fn main(){let outside=7;assert_eq!(host::call_capture!(),7);}",
            true,
        ),
        (
            "borrow_escape",
            "fn escaped()->&'static u32{let x=7;host::identity!(&x)}fn main(){}",
            false,
        ),
        (
            "type_mismatch",
            "fn main(){let x:u32=host::identity!(false);}",
            false,
        ),
        (
            "no_std_does_not_load_std",
            "#![no_std] fn value(x:f64)->f64{host::identity!(x).sqrt()}fn main(){}",
            false,
        ),
    ].into_iter().map(|(name,text,expected)|(name,text,expected,expected)).chain([
        ("generated_builtin_attribute", "#[derive(host::Generated)]#[repr(C)]struct S;fn main(){}", true, false),
        ("generated_field_attribute", "#[derive(host::Generated)]struct S{#[allow(dead_code)]n:u32}fn main(){}", true, false),
        // Generated helper declarations are not present in the host catalog.
        // Keep this Rust-valid form explicitly unsupported rather than treating
        // every unrecognized attribute as a helper.
        ("generated_helper_metadata", "#[derive(host::GeneratedHelper)]#[generated_helper_attr]struct S;fn main(){}", true, false),
    ]) {
        let file = root.join(format!("{name}.rs"));
        std::fs::write(&file, text).unwrap();
        let output = run(
            Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--crate-type=lib",
                    "--emit=metadata",
                    "--extern",
                ])
                .arg(format!("host={}", dylib.display()))
                .arg(&file)
                .arg("-o")
                .arg(root.join(format!("{name}.rmeta"))),
            &root,
            &format!("rustc-{name}"),
        );
        assert_eq!(
            output.status.success(),
            expected,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let host = db.add_proc_macro_crate(dylib.clone(), exports.clone());
        let main = db.load(&file).unwrap();
        db.set_crate_directory(main, root.clone()).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: host,
                    name: Symbol::intern("host"),
                    dependencies: Vec::new(),
                },
                SourceCrate {
                    root: main,
                    name: Symbol::intern("probe"),
                    dependencies: vec![(Symbol::intern("host"), 0)],
                },
            ],
        );
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics.clone());
        if diagnostics.is_empty() {
            let typed = flux_mir::check_file(&hir);
            diagnostics.extend(typed.diagnostics.clone());
            if diagnostics.is_empty() {
                let mir = flux_mir::build_file(&hir, &typed);
                assert!(mir.bugs.is_empty(), "{name}: {:?}", mir.bugs);
                diagnostics.extend(mir.diagnostics);
            }
        }
        if matches!(name, "generated_helper_metadata" | "generated_helper_macro_shadow" | "generated_builtin_attribute" | "generated_field_attribute") {
            assert!(diagnostics.iter().any(|d| d.message.contains("attribute")), "helper metadata boundary: {diagnostics:?}");
        }
        assert_eq!(
            diagnostics.is_empty(),
            flux_expected,
            "Flux {name}: {diagnostics:?}"
        );
    }
}
