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
#[proc_macro] pub fn size_of_type(input:TokenStream)->TokenStream {
    let mut tokens = input.into_iter();
    let Some(TokenTree::Group(ty)) = tokens.next() else { panic!("expected a grouped type") };
    assert_eq!(ty.delimiter(), proc_macro::Delimiter::None);
    assert!(tokens.next().is_none());
    let mut output: TokenStream = "::core::mem::size_of::<".parse().unwrap();
    output.extend([TokenTree::Group(ty)]);
    output.extend(">()".parse::<TokenStream>().unwrap());
    output
}
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
#[proc_macro_attribute] pub fn host(_:TokenStream,_:TokenStream)->TokenStream {
    "fn from_attribute() -> u32 { 37 }".parse().unwrap()
}
#[proc_macro] pub fn failure(_:TokenStream)->TokenStream {"compile_error!(\"macro rejected input\");".parse().unwrap()}
#[proc_macro] pub fn panics(_:TokenStream)->TokenStream {panic!("deliberate macro panic")}
#[proc_macro_attribute] pub fn add(arguments:TokenStream,input:TokenStream)->TokenStream {
    let mut output=input;output.extend(format!("fn added()->usize{{ {arguments} }}").parse::<TokenStream>().unwrap());output
}
#[proc_macro_derive(Answer,attributes(answer))] pub fn answer(input:TokenStream)->TokenStream {
    assert!(input.to_string().contains("answer"),"helper input lost");
    assert!(!input.to_string().contains("cfg_attr"),"derive input is not configured");
    assert!(!input.to_string().contains("missing"),"disabled field leaked to derive");
    let mut iter=input.into_iter();let mut name=None;
    while let Some(t)=iter.next(){if matches!(&t,TokenTree::Ident(i) if i.to_string()=="struct"){name=iter.next().map(|t|t.to_string());break;}}
    format!("impl {}{{fn answer()->usize{{42}}}}",name.unwrap()).parse().unwrap()
}
#[proc_macro_derive(ConfiguredEnum, attributes(answer))]
pub fn configured_enum(input: TokenStream) -> TokenStream {
    let text = input.to_string();
    assert_eq!(text.matches("answer").count(), 2);
    assert!(text.contains("enabled variant"));
    assert!(!text.contains("cfg_attr"));
    assert!(!text.contains("Unavailable"));
    "impl Status { fn available() -> Self { Self::Available(17) } }".parse().unwrap()
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
struct Item {
    #[cfg_attr(all(), cfg_attr(all(), answer))]
    #[cfg_attr(any(), invented_helper)]
    value:usize,
    #[cfg(any())] missing:MissingType
}
#[derive(host::ConfiguredEnum)]
enum Status {
    #[cfg_attr(all(), answer, doc = "enabled variant")]
    Available(#[cfg_attr(all(), answer)] u32),
    #[cfg_attr(all(), cfg(any()))]
    Unavailable(MissingType),
}
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
    #[derive(Clone, Answer)]
    #[answer]
    struct LateDerived;
    pub fn derived() -> usize { LateDerived::answer() }
    #[crate::late_prelude::docs]
    #[doc="retained"]
    pub fn attributed() -> usize { 47 }
    #[crate::late_prelude::remove]
    fn erased(_: MissingType) {}
    impl Late { identity! { pub fn value() -> usize { 29 } } }
}
mod late_prelude { pub use host::{identity, docs, remove, Answer}; }
mod inherited_attribute {
    use host::*;
    mod nested {
        use super::*;
        #[host]
        fn replaced() {}
        pub(super) fn value() -> u32 { from_attribute() }
        pub(super) fn block_value() -> u32 {
            #[host]
            unsafe extern "C" {}
            from_attribute()
        }
    }
    pub fn value() -> u32 { nested::value() }
    pub fn block_value() -> u32 { nested::block_value() }
}
fn main(){
    assert_eq!(inherited_attribute::value(),37);
    assert_eq!(inherited_attribute::block_value(),37);
    let qualified=Qualified(37); assert_eq!(qualified.clone().0,37);
    assert_eq!(forwarded_local!(17),17);
    assert_eq!(anonymous(&3,7),7);
    assert_eq!(later_consumer::Late::value(),29);
    assert_eq!(later_consumer::derived(),42);
    assert_eq!(later_consumer::attributed(),47);
    assert_eq!(GeneratedItem::generated(),43);assert_eq!(generated_attribute_value(),31);
    let a=host::counter!();let b=host::counter!();assert_ne!(a,b);assert_eq!(a+b,1);
    let value=Item{value:3};assert_eq!(value.clone().value,3);assert_eq!(Item::answer(),42);
    let Status::Available(status)=Status::available();assert_eq!(status,17);
    assert_eq!(original()+added()+generated(),59);
    assert_eq!(documented(),23);let _=Documented;
    assert!(host::suffix!("word"custom));assert!(host::emit_suffix!());
    macro_rules! separated {($l:literal,$i:ident)=>{host::separate!($l $i)}}
    assert!(separated!("word",custom));
    macro_rules! forwarded_type {($ty:ty)=>{host::size_of_type!($ty)}}
    assert_eq!(forwarded_type!(u16),2);
    assert_eq!(forwarded_type!(&'static [u8]),16);
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
            .args(["build", "--cargo", "--offline", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app"]);
        if optimize {
            command.arg("-O");
        }
        let output = run(&mut command, &root, &format!("flux-build-{optimize}"));
        support::oracle::success(&output, "Flux macros build");
        let output = run(
            &mut Command::new(root.join(if optimize {
                "target/flux/release/app"
            } else {
                "target/flux/debug/app"
            })),
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
            .as_ref()
            .is_some_and(Vec::is_empty)
    );
    assert!(
        exports
            .iter()
            .find(|m| m.name.as_str() == "GeneratedHelper")
            .unwrap()
            .helpers
            .as_deref()
            == Some(&[Symbol::intern("generated_helper_attr")])
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
    for (name, text, expected) in [
        (
            "associated_attributes_keep_owner",
            "struct S;\
             trait Tr { #[host::generated_attribute] fn value(&self)->usize { 7 } }\
             impl Tr for S {}\
             impl S { #[crate::later::add(9)] fn original()->usize { 11 }\
             #[crate::later::remove] fn erased(_:missing_type!()) {} }\
             mod later { pub use host::{add,remove}; }\
             fn main() { assert_eq!(S.value()+S::original()+S::added(),27); }",
            true,
        ),
        (
            "associated_attribute_duplicate",
            "struct S;trait Tr{fn added()->usize;fn original()->usize;}\
             impl Tr for S{#[host::add(9)]fn original()->usize{7}fn added()->usize{2}}\
             fn main(){}",
            false,
        ),
        (
            "associated_attribute_does_not_export",
            "struct S;impl S{#[host::add(9)]fn original()->usize{7}}fn main(){added();}",
            false,
        ),
        (
            "generated_function_helper_shadow",
            "use host::{GeneratedHelper,remove as generated_helper_attr};#[generated_helper_attr]#[derive(GeneratedHelper)]fn bad(){}fn main(){}",
            false,
        ),
        (
            "generated_derive_before_attribute",
            "#[derive(host::GeneratedHelper)]#[host::remove]fn bad(){}fn main(){}",
            false,
        ),
        (
            "generated_attribute_before_derive",
            "#[host::remove]#[derive(host::GeneratedHelper)]fn bad(){}fn main(){}",
            true,
        ),
        (
            "generated_disabled_variant",
            "#[derive(host::GeneratedHelper)]enum E{#[cfg(any())]Disabled{#[allow(dead_code)]value:Missing},Enabled}fn main(){}",
            true,
        ),
        (
            "generated_qualified_attribute",
            "#[derive(host::Generated)]#[host::generated_attribute]struct S;fn main(){assert_eq!(S::generated(),43);}",
            true,
        ),
        (
            "generated_configured_fields",
            "#[cfg_attr(all(),derive(host::Generated))]struct S{#[cfg(any())]#[allow(dead_code)]missing:Missing,n:u32}fn main(){assert_eq!(S::generated(),43);}",
            true,
        ),
        (
            "generated_helper_macro_shadow",
            "use host::{GeneratedHelper, add as generated_helper_attr};#[derive(GeneratedHelper)]#[generated_helper_attr(37)]struct S;fn main(){let _=added();}",
            false,
        ),
        (
            "generated_derive",
            "#[derive(host::Generated)]struct S;fn main(){assert_eq!(S::generated(),43);}",
            true,
        ),
        (
            "generated_builtin_attribute",
            "#[derive(host::Generated)]#[repr(C)]struct S;fn main(){}",
            true,
        ),
        (
            "generated_field_attribute",
            "#[derive(host::Generated)]struct S{#[allow(dead_code)]n:u32}fn main(){}",
            true,
        ),
        (
            "generated_helper_metadata",
            "#[derive(host::GeneratedHelper)]#[generated_helper_attr]struct S;fn main(){}",
            true,
        ),
        (
            "generated_field_helper_metadata",
            "#[derive(host::GeneratedHelper)]struct S{#[generated_helper_attr]n:u32}fn main(){}",
            true,
        ),
        (
            "generated_undeclared_helper",
            "#[derive(host::Generated)]#[generated_helper_attr]struct S;fn main(){}",
            false,
        ),
        (
            "generated_qualified_helper",
            "#[derive(host::GeneratedHelper)]#[generated_helper_attr::wrong]struct S;fn main(){}",
            false,
        ),
        (
            "generated_unsafe_helper",
            "#[derive(host::GeneratedHelper)]#[unsafe(generated_helper_attr)]struct S;fn main(){}",
            false,
        ),
        (
            "generated_attribute",
            "#[host::generated_attribute]fn f(){}fn main(){f();}",
            true,
        ),
        (
            "generated_function",
            "host::generated_identity!(fn f(){});fn main(){f();}",
            true,
        ),
        (
            "generated_unknown_helper",
            "#[derive(host::Generated)]#[invented_helper]struct S;fn main(){}",
            false,
        ),
        (
            "disabled_generated_export",
            "#[derive(host::Disabled)]struct S;fn main(){}",
            false,
        ),
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
            "attribute_glob_named_like_extern_crate",
            "use host::*;#[host]fn replaced(){}fn main(){assert_eq!(from_attribute(),37);}",
            true,
        ),
        (
            "attribute_glob_named_like_local_module",
            "mod host{}use ::host::*;#[host]fn replaced(){}fn main(){assert_eq!(from_attribute(),37);}",
            true,
        ),
        (
            "attribute_glob_through_parent_scope",
            "use host::*;mod parent{use super::*;mod child{use super::*;#[host]fn replaced(){}pub fn check(){assert_eq!(from_attribute(),37);}}pub fn check(){child::check();}}fn main(){parent::check();}",
            true,
        ),
        (
            "attribute_glob_through_crate_scope",
            "use host::*;mod child{use crate::*;#[host]fn replaced(){}pub fn check(){assert_eq!(from_attribute(),37);}}fn main(){child::check();}",
            true,
        ),
        (
            "attribute_generated_item_stays_in_block",
            "use host::*;fn nested(){#[host]unsafe extern \"C\" {}let _=from_attribute();}fn main(){let _=from_attribute();}",
            false,
        ),
        (
            "derive_from_parent_glob",
            "mod child{use super::*;#[cfg_attr(all(),derive(Clone,Build),answer)]struct S;fn check(){let _=S.clone();assert_eq!(S::answer(),42);}}pub use host::Answer as Build;",
            true,
        ),
        (
            "derive_from_private_glob",
            "mod owner{use host::Answer;}mod child{use super::owner::*;#[derive(Answer)]#[answer]struct S;}",
            false,
        ),
        (
            "derive_from_ambiguous_globs",
            "mod first{pub use host::Answer as Build;}mod second{pub use host::Generated as Build;}mod child{use super::{first::*,second::*};#[derive(Build)]struct S;}",
            false,
        ),
        (
            "helper_field",
            "#[derive(host::Answer)]struct S{#[answer]n:u32}fn main(){}",
            true,
        ),
        (
            "active_helper_field",
            "#[derive(host::Answer)]struct S(#[cfg_attr(all(),cfg_attr(all(),answer))]u32);fn main(){assert_eq!(S::answer(),42);}",
            true,
        ),
        (
            "active_unknown_field_attribute",
            "#[derive(host::Answer)]#[answer]struct S(#[cfg_attr(all(),unknown_helper)]u32);fn main(){}",
            false,
        ),
        (
            "active_unsafe_field_helper",
            "#[derive(host::Answer)]#[answer]struct S(#[cfg_attr(all(),unsafe(answer))]u32);fn main(){}",
            false,
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
    ] {
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
        assert_eq!(
            diagnostics.is_empty(),
            expected,
            "Flux {name}: {diagnostics:?}"
        );
    }
}
