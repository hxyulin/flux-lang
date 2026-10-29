//! Host descriptors restore execution without revisiting procedural-macro source.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use std::path::PathBuf;

use flux_cli::flib::semantic::{CheckedCrate, Dependency, load_checked, write_checked};
use flux_hir::flib::CrateIndices;
use flux_hir::{CrateId, ProcMacroInput, ProcMacroResolver, SourceCrate, SourceDb};
use flux_intern::Symbol;

struct Fixture {
    _directory: scratch::Scratch,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = scratch::Scratch::new(std::env::temp_dir().join("flux-flib-proc"));
        let path = directory.path().canonicalize().unwrap();
        Self {
            _directory: directory,
            path,
        }
    }
    fn input(&self, value: &str) -> ProcMacroInput {
        let mut db = SourceDb::new();
        let root = db.add(self.path.join("consumer.rs"), "");
        ProcMacroInput {
            invocation_crate: 17,
            tokens: vec![],
            attributes: None,
            source: db.shared(root),
            call: flux_syntax::TextRange::empty(0),
            range_base: 0,
            mixed_context: 19,
            environment: std::collections::BTreeMap::from([(
                "FLIB_MACRO_VALUE".into(),
                value.into(),
            )]),
            current_dir: self.path.clone(),
        }
    }
}
fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: vec![],
        bugs: vec![],
    }
}

#[test]
fn local_host_descriptor_roundtrips_metadata_reexports_and_rejects_stale_binary() {
    let fixture = Fixture::new();
    let source = fixture.path.join("macros.rs");
    std::fs::write(&source,r#"
extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro] pub fn answer(_:TokenStream)->TokenStream{"42u32".parse().unwrap()}
#[proc_macro_attribute] pub fn keep(_:TokenStream,item:TokenStream)->TokenStream{item}
#[proc_macro_derive(Demo,attributes(allowed))] pub fn demo(_:TokenStream)->TokenStream{TokenStream::new()}
#[proc_macro] pub fn environment(_:TokenStream)->TokenStream{format!("{:?}",std::env::var("FLIB_MACRO_VALUE").unwrap()).parse().unwrap()}
"#).unwrap();
    let output = fixture.path.join("built");
    let exports = flux_cli::proc_macro::local::build_host(
        &source,
        "flib_macros",
        flux_syntax::Edition::E2024,
        &output,
        &[],
        &[],
        &fixture.path,
    )
    .unwrap();
    let descriptor = exports
        .iter()
        .find(|e| e.name.as_str() == "environment")
        .unwrap()
        .expander
        .flib_descriptor()
        .unwrap();
    let resolver = flux_cli::proc_macro::FlibHostResolver::default();
    let restored = resolver.resolve(&descriptor).unwrap();
    for value in ["first", "second"] {
        let text = restored
            .expand(fixture.input(value))
            .unwrap()
            .into_iter()
            .map(|t| t.text)
            .collect::<String>();
        assert_eq!(text, format!("{value:?}"));
    }
    assert_eq!(restored.flib_descriptor().unwrap(), descriptor);
    let mut db = SourceDb::new();
    let root = db.add_proc_macro_crate(source.clone(), exports);
    let hir = flux_hir::lower_sources(
        &mut db,
        &[SourceCrate {
            root,
            name: Symbol::intern("macros"),
            dependencies: vec![],
        }],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let owner = hir.items.main_crate();
    let dependencies: Vec<_> = hir
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != owner.0 as usize)
        .map(|(index, data)| Dependency {
            krate: CrateId(index as u32),
            hash: flux_hir::flib::digest(data.name.as_str().as_bytes()),
            indices: CrateIndices::collect(&hir.items, CrateId(index as u32)),
        })
        .collect();
    let bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: owner,
        hash: [8; 32],
        dependencies: &dependencies,
        object: None,
        freshness: None,
    })
    .unwrap();
    std::fs::remove_file(&source).unwrap();
    let mut db = SourceDb::new();
    let mut items = flux_hir::Core::get().items.clone();
    items.core = items.counts();
    let mut sigs = flux_ty::LibraryTypes::get().sigs.clone();
    let mut mir = empty_mir();
    let loaded = load_checked(
        &bytes,
        &mut db,
        &mut items,
        &mut sigs,
        &mut mir,
        &dependencies,
    )
    .unwrap();
    let relay = db.add("relay.rs", "pub use macros::{answer,keep,Demo};");
    let relay = flux_hir::lower_source_on(
        &mut db,
        items,
        relay,
        Symbol::intern("relay"),
        vec![(Symbol::intern("macros"), loaded.dependency.krate)],
    );
    assert!(relay.diagnostics.is_empty(), "{:?}", relay.diagnostics);
    let relay_id = relay.items.main_crate();
    let root = db.add(
        "consumer.rs",
        r#"
        #[relay::keep] fn value()->u32{relay::answer!()}
        #[derive(relay::Demo)] #[allowed] struct Kept;
        fn main(){let _=value();}
    "#,
    );
    let consumer = flux_hir::lower_source_on(
        &mut db,
        relay.items,
        root,
        Symbol::intern("consumer"),
        vec![(Symbol::intern("relay"), relay_id)],
    );
    assert!(
        consumer.diagnostics.is_empty(),
        "{:?}",
        consumer.diagnostics
    );
    assert!(!source.exists());
    std::fs::write(output.join("host"), b"changed host").unwrap();
    let error = resolver.resolve(&descriptor).unwrap_err();
    assert!(error.contains("host changed"), "{error}");
    assert!(
        restored
            .expand(fixture.input("third"))
            .unwrap_err()
            .contains("host changed")
    );
    for end in [0, 1, descriptor.len() - 1] {
        assert!(resolver.resolve(&descriptor[..end]).is_err());
    }
}

#[test]
fn rustc_host_descriptors_share_state_only_inside_one_invocation_crate_and_graph() {
    let fixture = Fixture::new();
    let source = fixture.path.join("state.rs");
    let dylib = fixture.path.join("libstate.dylib");
    std::fs::write(&source,r#"
extern crate proc_macro;
use proc_macro::TokenStream;
use std::sync::atomic::{AtomicUsize,Ordering};
static COUNT:AtomicUsize=AtomicUsize::new(0);
#[proc_macro]pub fn counter(_:TokenStream)->TokenStream{(COUNT.fetch_add(1,Ordering::SeqCst)+1).to_string().parse().unwrap()}
#[proc_macro]pub fn report(_:TokenStream)->TokenStream{COUNT.load(Ordering::SeqCst).to_string().parse().unwrap()}
"#).unwrap();
    let output = std::process::Command::new("rustc")
        .args(["--edition=2024", "--crate-type=proc-macro"])
        .arg(&source)
        .arg("-o")
        .arg(&dylib)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .unwrap();
    assert!(version.status.success());
    let exports = flux_cli::proc_macro::exports(
        &String::from_utf8(version.stdout).unwrap(),
        &dylib,
        &source,
        flux_hir::CrateCfg {
            names: vec![Symbol::intern("proc_macro")],
            values: vec![],
        },
    )
    .unwrap();
    let descriptor = |name: &str| {
        exports
            .iter()
            .find(|export| export.name.as_str() == name)
            .unwrap()
            .expander
            .flib_descriptor()
            .unwrap()
    };
    let counter_bytes = descriptor("counter");
    let report_bytes = descriptor("report");
    std::fs::remove_file(&source).unwrap();
    let resolver = flux_cli::proc_macro::FlibHostResolver::default();
    let counter = resolver.resolve(&counter_bytes).unwrap();
    let report = resolver.resolve(&report_bytes).unwrap();
    let expand = |host: &dyn flux_hir::ProcMacroExpander, krate| {
        let mut input = fixture.input("");
        input.invocation_crate = krate;
        host.expand(input)
            .unwrap()
            .into_iter()
            .map(|token| token.text)
            .collect::<String>()
    };
    assert_eq!(expand(counter.as_ref(), 17), "1");
    assert_eq!(expand(report.as_ref(), 17), "1");
    assert_eq!(expand(counter.as_ref(), 17), "2");
    assert_eq!(expand(report.as_ref(), 17), "2");
    assert_eq!(expand(report.as_ref(), 18), "0");
    let other = flux_cli::proc_macro::FlibHostResolver::default();
    let independent = other.resolve(&report_bytes).unwrap();
    assert_eq!(expand(independent.as_ref(), 17), "0");
    // Replace the path without truncating the already mapped dynamic library.
    std::fs::rename(&dylib, fixture.path.join("original.dylib")).unwrap();
    std::fs::write(&dylib, b"changed dynamic library").unwrap();
    assert!(
        resolver
            .resolve(&report_bytes)
            .unwrap_err()
            .contains("host changed")
    );
    assert!(
        report
            .expand(fixture.input(""))
            .unwrap_err()
            .contains("host changed")
    );
}

#[test]
fn body_only_host_observations_change_identity_with_identical_source_and_host() {
    #[derive(Debug)]
    struct Observed(std::sync::atomic::AtomicU32);
    impl flux_hir::ProcMacroExpander for Observed {
        fn expand(
            &self,
            mut input: ProcMacroInput,
        ) -> Result<Vec<flux_hir::ProcMacroToken>, String> {
            input.tokens[0].text = self
                .0
                .load(std::sync::atomic::Ordering::Relaxed)
                .to_string();
            Ok(input.tokens)
        }
        fn flib_descriptor(&self) -> Option<Vec<u8>> {
            Some(b"unchanged-host".to_vec())
        }
    }
    let host = std::sync::Arc::new(Observed(std::sync::atomic::AtomicU32::new(1)));
    let compile = || {
        let mut db = SourceDb::new();
        let macros = db.add_proc_macro_crate(
            "same-host.rs".into(),
            vec![flux_hir::ProcMacro {
                name: Symbol::intern("observed"),
                kind: flux_hir::ProcMacroKind::Function,
                helpers: Some(vec![]),
                expander: host.clone(),
            }],
        );
        let source = db.add("same-consumer.rs", "fn main(){let _=host::observed!(0);}");
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: macros,
                    name: Symbol::intern("host"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: source,
                    name: Symbol::intern("consumer"),
                    dependencies: vec![(Symbol::intern("host"), 0)],
                },
            ],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let own = hir.items.main_crate();
        let inputs = hir.items.crates[own.0 as usize].tracked_inputs();
        assert_eq!(inputs.macro_outputs.len(), 1);
        let dependencies: Vec<_> = hir
            .items
            .crates
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != own.0 as usize)
            .map(|(index, data)| {
                let krate = CrateId(index as u32);
                Dependency {
                    krate,
                    hash: flux_hir::flib::digest(data.name.as_str().as_bytes()),
                    indices: CrateIndices::collect(&hir.items, krate),
                }
            })
            .collect();
        flux_cli::flib::semantic::crate_hash(
            &hir.items,
            own,
            &dependencies,
            b"unchanged configuration",
        )
        .unwrap()
    };
    let first = compile();
    host.0.store(2, std::sync::atomic::Ordering::Relaxed);
    let second = compile();
    assert_ne!(first, second);
    assert_eq!(
        second,
        compile(),
        "fresh process-local hygiene identities must hash canonically"
    );
}
