//! Strong-count reservation transfers payload ownership without excluding weak handles.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn arc_try_unwrap_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("arc_try_unwrap");
}

#[test]
fn production_arc_try_unwrap_host_races_and_unwind() {
    use std::{path::Path, process::Command};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/arc-try-unwrap-host/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let production = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/alloc/src/sync.rs"),
    )
    .unwrap();
    // Host-only adaptation removes Flux's lang registration and redirects its
    // private abort hook. All allocation/refcount/payload code stays unchanged.
    std::fs::write(
        root.join("implementation.rs"),
        production
            .replace("#[lang = \"arc\"]", "")
            .replace("core::panicking::abort_now()", "std::process::abort()"),
    )
    .unwrap();
    let source = r#"
extern crate alloc as host_alloc;
pub use host_alloc::{vec,boxed,string,borrow};
use std::sync::atomic::{AtomicUsize,Ordering::SeqCst};
static ALLOCS:AtomicUsize=AtomicUsize::new(0);static DEALLOCS:AtomicUsize=AtomicUsize::new(0);
pub mod alloc{pub use std::alloc::{Layout,handle_alloc_error};pub unsafe fn alloc(layout:Layout)->*mut u8{super::ALLOCS.fetch_add(1,super::SeqCst);unsafe{std::alloc::alloc(layout)}}pub unsafe fn dealloc(ptr:*mut u8,layout:Layout){super::DEALLOCS.fetch_add(1,super::SeqCst);unsafe{std::alloc::dealloc(ptr,layout)}}}
mod raw_vec{pub fn capacity_overflow()->!{panic!("capacity overflow")}}
mod implementation;
use implementation::Arc;
struct Owner{drops:std::sync::Arc<AtomicUsize>,panic:bool}
impl Drop for Owner{fn drop(&mut self){self.drops.fetch_add(1,SeqCst);assert!(!self.panic,"payload panic");}}
fn main(){
 let drops=std::sync::Arc::new(AtomicUsize::new(0));
 let a=Arc::new(Owner{drops:drops.clone(),panic:false});let weak=Arc::downgrade(&a);
 let value=Arc::try_unwrap(a).ok().unwrap();assert!(weak.upgrade().is_none());assert_eq!(DEALLOCS.load(SeqCst),0);assert_eq!(drops.load(SeqCst),0);drop(value);assert_eq!(drops.load(SeqCst),1);drop(weak);assert_eq!(DEALLOCS.load(SeqCst),1);
 let a=Arc::new(Owner{drops:drops.clone(),panic:true});let weak=Arc::downgrade(&a);let value=Arc::try_unwrap(a).ok().unwrap();assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(||drop(value))).is_err());assert!(weak.upgrade().is_none());assert_eq!(drops.load(SeqCst),2);drop(weak);
 for _ in 0..500{
   let a=Arc::new(Owner{drops:drops.clone(),panic:false});let weak=Arc::downgrade(&a);let gate=std::sync::Barrier::new(2);
   std::thread::scope(|scope|{let gate=&gate;let left=scope.spawn(move||{gate.wait();match Arc::try_unwrap(a){Ok(value)=>drop(value),Err(a)=>drop(a)}});let right=scope.spawn(move||{gate.wait();if let Some(value)=weak.upgrade(){assert!(!value.panic);drop(value);}});left.join().unwrap();right.join().unwrap();});
 }
 assert_eq!(drops.load(SeqCst),502);assert_eq!(ALLOCS.load(SeqCst),DEALLOCS.load(SeqCst));
 println!("production unwrap 500 races 502 drops");
}
"#;
    std::fs::write(root.join("probe.rs"), source).unwrap();
    for optimized in [false, true] {
        let exe = root.join(format!("probe-{optimized}"));
        let mut command = Command::new("rustc");
        command
            .args(["--edition=2024", "-Awarnings"])
            .arg(root.join("probe.rs"))
            .arg("-o")
            .arg(&exe);
        if optimized {
            command.arg("-O");
        }
        let output = support::process::run(
            &mut command,
            &root,
            &format!("host-build-{optimized}"),
            std::time::Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "host production Arc build");
        let output = support::process::run(
            &mut Command::new(exe),
            &root,
            &format!("host-run-{optimized}"),
            std::time::Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "host production Arc race/unwind");
        assert_eq!(output.stdout, b"production unwrap 500 races 502 drops\n");
    }
}
