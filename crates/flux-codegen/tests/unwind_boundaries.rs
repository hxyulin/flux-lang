//! Native process entries and foreign ABI boundaries use the Flux panic runtime.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use flux_codegen::{NativeLink, OptimizationTier, Target};
use flux_hir::{NativeKind, NativeLibrary};
use flux_intern::Symbol;
use flux_mir::PanicStrategy;
use std::process::Command;
use std::time::Duration;
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

struct Program {
    name: &'static str,
    source: &'static str,
    cases: &'static [(&'static str, Option<i32>)],
}

const PROGRAMS: &[Program] = &[
    Program {
        name: "abi",
        source: r#"
use std::panic::{catch_unwind,panic_any};
use std::sync::atomic::{AtomicUsize,Ordering};
static DROPS:AtomicUsize=AtomicUsize::new(0);
struct Guard;
impl Drop for Guard{fn drop(&mut self){DROPS.fetch_add(1,Ordering::SeqCst);eprintln!("event local {}",std::thread::panicking());}}
extern "C" fn boundary(){let _guard=Guard;panic!("C boundary");}
extern "C-unwind" fn permitted(){let _guard=Guard;panic_any(17usize);}
#[repr(C)]#[derive(Debug)]struct Triple{a:i64,b:f64,c:i64}
extern "C-unwind" fn aggregate(value:Triple,scale:f64)->Triple{
    let _guard=Guard;
    if value.a<0{panic_any(value.a);}
    Triple{a:value.a+17,b:value.b*scale,c:value.c+1}
}
#[inline(never)]fn indirect(callback:extern "C-unwind" fn(Triple,f64)->Triple,value:i64)->Triple{
    callback(Triple{a:value,b:1.25,c:19},2.5)
}
fn main(){
    std::panic::set_hook(Box::new(|_|{}));
    match std::env::args().nth(1).as_deref(){
        Some("c")=>{let _=catch_unwind(||boundary());panic!("C panic escaped");}
        Some("c-pointer")=>{let f=std::hint::black_box(boundary as extern "C" fn());let _=catch_unwind(||f());panic!("C panic escaped");}
        Some("c-unwind")=>{
            let error=catch_unwind(||permitted()).unwrap_err();
            assert_eq!(*error.downcast_ref::<usize>().unwrap(),17);
            assert_eq!(DROPS.load(Ordering::SeqCst),1);
            assert!(!std::thread::panicking());println!("C-unwind passed");
        }
        Some("aggregate")=>{
            let f=std::hint::black_box(aggregate as extern "C-unwind" fn(Triple,f64)->Triple);
            let value=indirect(f,7);assert_eq!(value.a,24);assert_eq!(value.b,3.125);assert_eq!(value.c,20);
            let error=catch_unwind(||indirect(f,-7)).unwrap_err();
            assert_eq!(*error.downcast_ref::<i64>().unwrap(),-7);
            assert_eq!(DROPS.load(Ordering::SeqCst),2);println!("aggregate passed");
        }
        _=>panic!("expected a case"),
    }
}
"#,
        cases: &[
            ("c", None),
            ("c-pointer", None),
            ("c-unwind", Some(0)),
            ("aggregate", Some(0)),
        ],
    },
    Program {
        name: "result",
        source: r#"
struct Guard;
impl Drop for Guard{fn drop(&mut self){println!("event main drop {}",std::thread::panicking());}}
fn main()->Result<(),&'static str>{
    std::panic::set_hook(Box::new(|_|{}));
    match std::env::args().nth(1).as_deref(){
        Some("error")=>Err("main result error"),
        Some("panic")=>{let _guard=Guard;panic!("main result panic");}
        _=>{println!("result passed");Ok(())}
    }
}
"#,
        cases: &[("ok", Some(0)), ("error", Some(1)), ("panic", Some(101))],
    },
    Program {
        name: "report",
        source: r#"
struct Error;
impl std::fmt::Debug for Error{fn fmt(&self,_:&mut std::fmt::Formatter<'_>)->std::fmt::Result{panic!("report panic");}}
impl Drop for Error{fn drop(&mut self){println!("event report drop {}",std::thread::panicking());}}
fn main()->Result<(),Error>{std::panic::set_hook(Box::new(|_|{}));Err(Error)}
"#,
        cases: &[("report", Some(101))],
    },
    Program {
        name: "never",
        source: r#"
struct Guard;
impl Drop for Guard{fn drop(&mut self){println!("event never local {}",std::thread::panicking());}}
struct Payload(bool);
impl Drop for Payload{fn drop(&mut self){eprintln!("event payload {}",std::thread::panicking());if self.0{panic!("payload drop panic");}}}
fn main()->!{
    std::panic::set_hook(Box::new(|_|{}));
    let fail=std::env::args().nth(1).as_deref()==Some("drop-panic");
    let _guard=Guard;
    std::panic::panic_any(Payload(fail));
}
"#,
        cases: &[("never", Some(101)), ("drop-panic", None)],
    },
    Program {
        name: "exit",
        source: r#"fn main()->std::process::ExitCode{std::process::ExitCode::from(37u8)}"#,
        cases: &[("exit", Some(37))],
    },
];

#[test]
fn entries_and_c_abi_boundaries_match_rustc() {
    let scratch = scratch::Scratch::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/unwind-boundaries"),
    );
    let root = scratch.path();
    let native = NativeLink::libraries(&[NativeLibrary {
        name: Symbol::intern("c++"),
        kind: NativeKind::Dylib,
    }]);
    for program in PROGRAMS {
        let input = root.join(format!("{}.rs", program.name));
        std::fs::write(&input, program.source).unwrap();
        let rust = root.join(format!("{}-rust", program.name));
        let output = process::run(
            Command::new("rustc")
                .args(["--edition=2024", "-C", "panic=unwind"])
                .arg(&input)
                .arg("-o")
                .arg(&rust),
            root,
            &format!("{}-rust-build", program.name),
            Duration::from_secs(60),
        )
        .completed();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Vec<_> = program
            .cases
            .iter()
            .map(|&(case, code)| {
                let output = process::run(
                    Command::new(&rust).arg(case),
                    root,
                    &format!("{}-rust-{case}", program.name),
                    Duration::from_secs(30),
                )
                .completed();
                assert_eq!(output.status.code(), code);
                (case, output)
            })
            .collect();
        let (_, _, hir) = flux_hir::lower_text(&input, program.source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        for (name, tier) in [
            ("none", OptimizationTier::None),
            ("full", OptimizationTier::Full),
        ] {
            let object = flux_codegen::compile_object_with_panic_strategy(
                &mut module.clone(),
                Target::Aarch64AppleDarwin,
                tier,
                PanicStrategy::Unwind,
            )
            .unwrap();
            let binary = root.join(format!("{}-{name}", program.name));
            flux_codegen::link_executable_with_libraries(
                &object,
                Target::Aarch64AppleDarwin,
                &binary,
                &native,
            )
            .unwrap();
            for (case, expected) in &expected {
                let output = process::run(
                    Command::new(&binary).arg(case),
                    root,
                    &format!("{}-{name}-{case}", program.name),
                    Duration::from_secs(30),
                )
                .completed();
                assert_eq!(
                    output.status,
                    expected.status,
                    "{} {name} {case}: {}",
                    program.name,
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(
                    output.stdout, expected.stdout,
                    "{} {name} {case}",
                    program.name
                );
                if expected.status.code().is_some() {
                    assert_eq!(
                        output.stderr, expected.stderr,
                        "{} {name} {case}",
                        program.name
                    );
                } else {
                    let events = |bytes: &[u8]| {
                        String::from_utf8_lossy(bytes)
                            .lines()
                            .filter(|line| line.starts_with("event "))
                            .map(str::to_owned)
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(
                        events(&output.stderr),
                        events(&expected.stderr),
                        "{} {name} {case}",
                        program.name
                    );
                }
            }
        }
    }
}
