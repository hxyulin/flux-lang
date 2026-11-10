//! Native Flux panics use Flux-owned payloads and catch boundaries.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]
use flux_codegen::{NativeLink, OptimizationTier, Target};
use flux_hir::{NativeKind, NativeLibrary};
use flux_intern::Symbol;
use flux_mir::PanicStrategy;
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;
use std::time::Duration;

const SOURCE: &str = r#"
use std::panic::{catch_unwind,panic_any,resume_unwind,AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize,Ordering};
static HOOKS:AtomicUsize=AtomicUsize::new(0);
static DROPS:AtomicUsize=AtomicUsize::new(0);
static PAYLOAD_DROPS:AtomicUsize=AtomicUsize::new(0);
struct Payload;
impl Drop for Payload{fn drop(&mut self){assert!(!std::thread::panicking());PAYLOAD_DROPS.fetch_add(1,Ordering::SeqCst);}}
struct Nested;
impl Drop for Nested{fn drop(&mut self){
    assert!(std::thread::panicking());
    assert!(catch_unwind(||panic!("nested")).is_err());
    assert!(std::thread::panicking());
}}
struct Owned;
impl Drop for Owned{fn drop(&mut self){assert!(std::thread::panicking());DROPS.fetch_add(1,Ordering::SeqCst);}}
fn main(){
    std::panic::set_hook(Box::new(|info|{
        assert!(std::thread::panicking());
        assert!(info.location().is_some());
        HOOKS.fetch_add(1,Ordering::SeqCst);
    }));
    match std::env::args().nth(1).as_deref(){
        Some("uncaught")=>{
            struct Event;
            impl Drop for Event{fn drop(&mut self){println!("uncaught cleanup");}}
            let _event=Event;
            panic!("uncaught");
        }
        Some("hook")=>{
            std::panic::set_hook(Box::new(|_|panic!("hook failure")));
            panic!("first");
        }
        Some("double")=>{
            struct Event(i32);
            impl Drop for Event{fn drop(&mut self){eprintln!("event {}",self.0);}}
            struct Fail;
            impl Drop for Fail{fn drop(&mut self){panic!("second");}}
            let _=catch_unwind(||{let _pair=(Event(1),Fail,Event(2));panic!("first");});
            unreachable!();
        }
        Some("format")=>{
            struct Broken;
            impl std::fmt::Display for Broken{
                fn fmt(&self,_:&mut std::fmt::Formatter<'_>)->std::fmt::Result{panic!("format failure");}
            }
            assert!(catch_unwind(||panic!("format {}",Broken)).is_err());
            println!("format caught");
            return;
        }
        _=>{}
    }
    assert_eq!(catch_unwind(||23).unwrap(),23);
    let payload=catch_unwind(||{let _owned=Owned;panic!("literal");}).unwrap_err();
    assert_eq!(payload.downcast_ref::<&'static str>().copied(),Some("literal"));
    assert!(!std::thread::panicking());
    assert_eq!(DROPS.load(Ordering::SeqCst),1);
    let payload=catch_unwind(||panic_any(71usize)).unwrap_err();
    let address=payload.downcast_ref::<usize>().unwrap() as *const usize as usize;
    let payload=catch_unwind(AssertUnwindSafe(||resume_unwind(payload))).unwrap_err();
    assert_eq!(*payload.downcast_ref::<usize>().unwrap(),71);
    assert_eq!(payload.downcast_ref::<usize>().unwrap() as *const usize as usize,address);
    assert_eq!(HOOKS.load(Ordering::SeqCst),2);
    let value=9;
    let payload=catch_unwind(||panic!("formatted {}",value)).unwrap_err();
    assert_eq!(payload.downcast_ref::<String>().unwrap(),"formatted 9");
    let mutex=std::sync::Mutex::new(0);
    assert!(catch_unwind(AssertUnwindSafe(||{let mut guard=mutex.lock().unwrap();*guard=7;panic!("poison");})).is_err());
    assert!(mutex.is_poisoned());
    assert_eq!(*mutex.lock().unwrap_err().into_inner(),7);
    assert!(!std::thread::panicking());
    assert_eq!(HOOKS.load(Ordering::SeqCst),4);
    assert!(catch_unwind(||{let _nested=Nested;panic!("outer");}).is_err());
    assert!(!std::thread::panicking());
    assert_eq!(HOOKS.load(Ordering::SeqCst),6);
    let payload=catch_unwind(||panic_any(Payload)).unwrap_err();
    assert!(payload.is::<Payload>());
    assert_eq!(PAYLOAD_DROPS.load(Ordering::SeqCst),0);
    drop(payload);
    assert_eq!(PAYLOAD_DROPS.load(Ordering::SeqCst),1);
    let old=std::panic::take_hook();
    std::panic::set_hook(Box::new(|info|{
        assert_eq!(info.payload().downcast_ref::<&str>().copied(),Some("replacement"));
        HOOKS.fetch_add(10,Ordering::SeqCst);
    }));
    assert!(catch_unwind(||panic!("replacement")).is_err());
    assert_eq!(HOOKS.load(Ordering::SeqCst),17);
    std::panic::set_hook(old);
    assert!(catch_unwind(||panic!("restored")).is_err());
    assert_eq!(HOOKS.load(Ordering::SeqCst),18);
    let ready=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let release=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    struct WaitDrop { ready:std::sync::Arc<std::sync::atomic::AtomicBool>, release:std::sync::Arc<std::sync::atomic::AtomicBool> }
    impl Drop for WaitDrop {fn drop(&mut self){
        assert!(std::thread::panicking());
        self.ready.store(true,Ordering::Release);
        while !self.release.load(Ordering::Acquire){std::thread::yield_now();}
    }}
    let child_ready=ready.clone();
    let child_release=release.clone();
    let child=std::thread::spawn(move||{
        let _wait=WaitDrop{ready:child_ready,release:child_release};
        panic_any(83usize);
    });
    while !ready.load(Ordering::Acquire){std::thread::yield_now();}
    assert!(!std::thread::panicking());
    release.store(true,Ordering::Release);
    assert_eq!(*child.join().unwrap_err().downcast::<usize>().unwrap(),83);
    let payload=std::thread::spawn(||panic_any(91usize)).join().unwrap_err();
    assert_eq!(*payload.downcast_ref::<usize>().unwrap(),91);
    assert!(!std::thread::panicking());
    let value=std::thread::scope(|scope|{
        let child=scope.spawn(||panic_any(String::from("scoped payload")));
        let payload=child.join().unwrap_err();
        assert_eq!(payload.downcast_ref::<String>().unwrap(),"scoped payload");
        17
    });
    assert_eq!(value,17);
    assert!(catch_unwind(||std::thread::scope(|scope|{scope.spawn(||panic!("unjoined"));})).is_err());
    let outer=catch_unwind(||std::thread::scope(|scope|{scope.spawn(||panic!("child"));panic_any(101usize);})).unwrap_err();
    assert_eq!(*outer.downcast_ref::<usize>().unwrap(),101);
    assert!(!std::thread::panicking());
    println!("owned payloads, resume, hooks, cleanup and poisoning passed");
}
"#;

#[test]
fn native_flux_runtime_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-unwind-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input.rs");
    std::fs::write(&input, SOURCE).unwrap();
    let rust = root.join("rust");
    let output = process::run(
        std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=unwind"])
            .arg(&input)
            .arg("-o")
            .arg(&rust),
        &root,
        "rust-build",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected_fatal: Vec<_> = ["uncaught", "hook", "double", "format"]
        .into_iter()
        .map(|mode| {
            let output = process::run(
                std::process::Command::new(&rust).arg(mode),
                &root,
                &format!("rust-{mode}"),
                Duration::from_secs(30),
            )
            .completed();
            (mode, output)
        })
        .collect();
    let expected = process::run(
        &mut std::process::Command::new(&rust),
        &root,
        "rust-run",
        Duration::from_secs(30),
    )
    .completed();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    let (db, _, hir) = flux_hir::lower_text(&input, SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap_or_else(|errors| {
        for error in &errors {
            eprintln!(
                "{} {:?}: {}",
                db.file(error.file).path.display(),
                error.range,
                error.message
            );
        }
        panic!("native lowering failed");
    });
    let native = NativeLink::libraries(&[NativeLibrary {
        name: Symbol::intern("c++"),
        kind: NativeKind::Dylib,
    }]);
    for (name, tier) in [
        ("none", OptimizationTier::None),
        ("full", OptimizationTier::Full),
    ] {
        let mut module = module.clone();
        let object = flux_codegen::compile_object_with_panic_strategy(
            &mut module,
            Target::Aarch64AppleDarwin,
            tier,
            PanicStrategy::Unwind,
        )
        .unwrap();
        let binary = root.join(name);
        flux_codegen::link_executable_with_libraries(
            &object,
            Target::Aarch64AppleDarwin,
            &binary,
            &native,
        )
        .unwrap();
        let output = process::run(
            &mut std::process::Command::new(&binary),
            &root,
            &format!("{name}-run"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            output.status,
            expected.status,
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout,
            expected.stdout,
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stderr, expected.stderr, "{name}");
        for (mode, expected) in &expected_fatal {
            let output = process::run(
                std::process::Command::new(&binary).arg(mode),
                &root,
                &format!("{name}-{mode}"),
                Duration::from_secs(30),
            )
            .completed();
            assert_eq!(
                output.status,
                expected.status,
                "{name} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected.stdout, "{name} {mode}");
            let actual = String::from_utf8(output.stderr).unwrap();
            let reference = String::from_utf8_lossy(&expected.stderr);
            let events: Vec<_> = actual
                .lines()
                .filter(|line| line.starts_with("event "))
                .collect();
            let wanted: Vec<_> = reference
                .lines()
                .filter(|line| line.starts_with("event "))
                .collect();
            assert_eq!(events, wanted, "{name} {mode}: {actual}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn abort_strategy_runs_the_hook_and_still_aborts() {
    let source = r#"
fn main(){
    std::panic::set_hook(Box::new(|info|{
        eprintln!("hook {} {}",info.payload().downcast_ref::<&str>().unwrap(),std::thread::panicking());
    }));
    panic!("abort probe");
}
"#;
    let root = std::env::temp_dir().join(format!("flux-abort-runtime-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("abort.rs");
    std::fs::write(&input, source).unwrap();
    let rust = root.join("rust");
    let output = process::run(
        std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&input)
            .arg("-o")
            .arg(&rust),
        &root,
        "rust-build",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = process::run(
        &mut std::process::Command::new(&rust),
        &root,
        "rust-run",
        Duration::from_secs(30),
    )
    .completed();
    let (_, _, hir) = flux_hir::lower_text(&input, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for (name, tier) in [
        ("none", OptimizationTier::None),
        ("full", OptimizationTier::Full),
    ] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        assert!(
            module
                .functions
                .iter()
                .all(|function| function.personality.is_none())
        );
        let object =
            flux_codegen::compile_object(&mut module, Target::Aarch64AppleDarwin, tier).unwrap();
        let binary = root.join(name);
        // Abort builds need no C++ personality library.
        flux_codegen::link_executable(&object, Target::Aarch64AppleDarwin, &binary).unwrap();
        let output = process::run(
            &mut std::process::Command::new(&binary),
            &root,
            name,
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(output.status, expected.status, "{name}");
        assert_eq!(output.stdout, expected.stdout, "{name}");
        assert_eq!(output.stderr, expected.stderr, "{name}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
