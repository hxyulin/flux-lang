//! Native exception propagation through Flux-generated cleanup graphs.
use flux_codegen::{OptimizationTier, Target};
use flux_mir::PanicStrategy;

const SOURCE: &str = r#"
unsafe extern "C-unwind" { fn raise_flux_probe(value:i32)->i32; }
unsafe extern "C" { fn cleanup_flux_probe(value:i32); }
struct Guard(i32);
impl Drop for Guard {fn drop(&mut self){unsafe{cleanup_flux_probe(self.0);}}}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_direct(value:i32)->i32 {
    let _guard=Guard(value.wrapping_add(10));
    unsafe{raise_flux_probe(value)}
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_indirect(callback:unsafe extern "C-unwind" fn(i32)->i32,value:i32)->i32 {
    let _guard=Guard(value.wrapping_add(10));
    unsafe{callback(value)}
}
trait Probe {fn apply(&self,value:i32)->i32;}
struct Callable;
impl Probe for Callable {fn apply(&self,value:i32)->i32{unsafe{raise_flux_probe(value)}}}
#[inline(never)]
fn virtual_call(callback:&dyn Probe,value:i32)->i32{callback.apply(value)}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_virtual(value:i32)->i32{
    let _guard=Guard(value.wrapping_add(10));
    virtual_call(&Callable,value)
}
#[inline(never)]
fn generic_call<F:Fn(i32)->i32>(callback:&F,value:i32)->i32{callback(value)}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_closure(value:i32)->i32{
    let _guard=Guard(value.wrapping_add(10));
    let delta=0;
    let callback=|value:i32|unsafe{raise_flux_probe(value.wrapping_add(delta))};
    generic_call(&callback,value)
}
#[repr(C)]
pub struct Triple {a:i64,b:f64,c:i64}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_aggregate(callback:unsafe extern "C-unwind" fn(Triple,f64)->Triple,value:i32)->i32{
    let _guard=Guard(value.wrapping_add(10));
    let result=unsafe{callback(Triple{a:value as i64,b:1.25,c:17},2.5)};
    result.a as i32
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_async(value:i32)->i32{
    let mut future=async move{
        let _guard=Guard(value.wrapping_add(10));
        unsafe{raise_flux_probe(value)}
    };
    let mut context=core::task::Context::from_waker(core::task::Waker::noop());
    let pinned=unsafe{core::pin::Pin::new_unchecked(&mut future)};
    match core::future::Future::poll(pinned,&mut context){
        core::task::Poll::Ready(value)=>value,
        core::task::Poll::Pending=>0,
    }
}
trait Marker {}
impl Marker for Guard {}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_box(value:i32)->i32{
    let _guard:Box<dyn Marker>=Box::new(Guard(value.wrapping_add(10)));
    unsafe{raise_flux_probe(value)}
}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_box_closure(value:i32)->i32{
    let guard=Guard(value.wrapping_add(10));
    let callback:Box<dyn FnOnce(i32)->i32>=Box::new(move|value|{
        let _guard=guard;
        unsafe{raise_flux_probe(value)}
    });
    callback(value)
}
struct PanickingGuard(Guard,i32);
impl Marker for PanickingGuard {}
impl Drop for PanickingGuard {fn drop(&mut self){unsafe{raise_flux_probe(self.1);}}}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_box_destructor(value:i32)->i32{
    let _guard:Box<dyn Marker>=Box::new(PanickingGuard(Guard(value.wrapping_add(10)),value));
    value.wrapping_mul(2)
}
struct Fail;
impl Drop for Fail{fn drop(&mut self){unsafe{raise_flux_probe(-8);}}}
#[unsafe(no_mangle)]
pub extern "C-unwind" fn flux_double(){
    let _pair=(Guard(4),Fail,Guard(5));
    unsafe{raise_flux_probe(-3);}
}
#[unsafe(no_mangle)]
pub extern "C" fn flux_boundary(value:i32)->i32{
    let _guard=Guard(value.wrapping_add(10));
    unsafe{raise_flux_probe(value)}
}
"#;

fn module() -> keel_ir::Module {
    let (_, _, hir) = flux_hir::lower_text("unwind_native.rs", SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap_or_else(|e| panic!("{e:?}\n{module}"));
    module
}

#[test]
fn exceptional_calls_lower_and_require_matching_object_emission() {
    let mut module = module();
    let text = module.to_string();
    assert!(text.contains("invoke @raise_flux_probe"), "{text}");
    assert!(text.contains("invoke @__flux_unwind_indirect_"), "{text}");
    assert!(text.contains("landingpad cleanup"), "{text}");
    assert!(text.contains("call @_Unwind_Resume"), "{text}");
    assert!(
        flux_codegen::compile_object(
            &mut module,
            Target::Aarch64AppleDarwin,
            OptimizationTier::None
        )
        .is_err()
    );
    let error = flux_codegen::compile_object_with_panic_strategy(
        &mut module,
        Target::X86_64LinuxGnu,
        OptimizationTier::None,
        PanicStrategy::Unwind,
    )
    .unwrap_err();
    assert!(error.to_string().contains("only for arm64 macOS"));
    flux_codegen::compile_object_with_panic_strategy(
        &mut module,
        Target::Aarch64AppleDarwin,
        OptimizationTier::None,
        PanicStrategy::Unwind,
    )
    .unwrap();
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_call_paths_preserve_payloads_and_live_cleanup_values() {
    let root = std::env::temp_dir().join(format!("flux-native-unwind-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let driver = root.join("driver.rs");
    std::fs::write(&driver,r#"
use std::sync::{Mutex,atomic::{AtomicUsize,Ordering}};
static LIVE:Mutex<Vec<usize>>=Mutex::new(Vec::new());
static ALLOCS:AtomicUsize=AtomicUsize::new(0);
unsafe extern "C" {
    fn malloc(size:usize)->*mut u8;
    fn calloc(count:usize,size:usize)->*mut u8;
    fn realloc(pointer:*mut u8,size:usize)->*mut u8;
    fn free(pointer:*mut u8);
    fn posix_memalign(out:*mut *mut u8,align:usize,size:usize)->i32;
}
fn allocated(pointer:*mut u8){
    if !pointer.is_null(){
        let mut live=LIVE.lock().unwrap();
        assert!(!live.contains(&(pointer as usize)));
        live.push(pointer as usize);
        ALLOCS.fetch_add(1,Ordering::Relaxed);
    }
}
fn released(pointer:*mut u8){
    if !pointer.is_null(){
        let mut live=LIVE.lock().unwrap();
        let index=live.iter().position(|p|*p==pointer as usize).expect("only free a live Flux allocation");
        live.swap_remove(index);
    }
}
#[unsafe(no_mangle)]
extern "C" fn flux_malloc_probe(size:usize)->*mut u8{
    // SAFETY: forward the allocator ABI and track the returned address.
    let pointer=unsafe{malloc(size)};allocated(pointer);pointer
}
#[unsafe(no_mangle)]
extern "C" fn flux_calloc_probe(count:usize,size:usize)->*mut u8{
    // SAFETY: forward the allocator ABI and track the returned address.
    let pointer=unsafe{calloc(count,size)};allocated(pointer);pointer
}
#[unsafe(no_mangle)]
extern "C" fn flux_realloc_probe(pointer:*mut u8,size:usize)->*mut u8{
    // SAFETY: Flux supplied a live allocation tracked by these wrappers.
    let result=unsafe{realloc(pointer,size)};
    if !result.is_null(){released(pointer);allocated(result);}result
}
#[unsafe(no_mangle)]
extern "C" fn flux_free_probe(pointer:*mut u8){
    released(pointer);
    // SAFETY: the pointer came from this allocator and is released once.
    unsafe{free(pointer)};
}
#[unsafe(no_mangle)]
extern "C" fn flux_posix_memalign_probe(out:*mut *mut u8,align:usize,size:usize)->i32{
    // SAFETY: Flux passes a writable pointer slot and valid allocator arguments.
    let result=unsafe{posix_memalign(out,align,size)};
    if result==0{
        // SAFETY: successful posix_memalign initialized the output slot.
        allocated(unsafe{*out});
    }result
}
static EVENTS:Mutex<Vec<i32>>=Mutex::new(Vec::new());
struct Guard(i32);
impl Drop for Guard{fn drop(&mut self){EVENTS.lock().unwrap().push(self.0);}}
unsafe extern "C-unwind" {
    fn flux_direct(value:i32)->i32;
    fn flux_virtual(value:i32)->i32;
    fn flux_closure(value:i32)->i32;
    fn flux_async(value:i32)->i32;
    fn flux_box(value:i32)->i32;
    fn flux_box_closure(value:i32)->i32;
    fn flux_box_destructor(value:i32)->i32;
    fn flux_double();
    fn flux_aggregate(callback:unsafe extern "C-unwind" fn(Triple,f64)->Triple,value:i32)->i32;
    fn flux_boundary(value:i32)->i32;
    fn flux_indirect(callback:unsafe extern "C-unwind" fn(i32)->i32,value:i32)->i32;
}
#[repr(C)]
struct Triple{a:i64,b:f64,c:i64}
extern "C-unwind" fn aggregate_probe(value:Triple,scale:f64)->Triple{
    assert_eq!(value.b,1.25);
    assert_eq!(value.c,17);
    assert_eq!(scale,2.5);
    Triple{a:raise_flux_probe(value.a as i32) as i64,b:9.5,c:71}
}
#[unsafe(no_mangle)]
extern "C-unwind" fn raise_flux_probe(value:i32)->i32{
    let _guard=Guard(1);
    if value<0{std::panic::panic_any(String::from("owned payload"));}
    value*2
}
#[unsafe(no_mangle)]
extern "C" fn cleanup_flux_probe(value:i32){
    EVENTS.lock().unwrap().push(value);
    if std::env::args().nth(1).is_some(){eprintln!("cleanup {value}");}
}
fn main(){
    std::panic::set_hook(Box::new(|_|{}));
    if std::env::args().nth(1).as_deref()==Some("boundary"){
        // SAFETY: this deliberately panics inside a non-unwinding Flux boundary.
        unsafe{flux_boundary(-3);}
        unreachable!();
    }
    if std::env::args().nth(1).as_deref()==Some("double"){
        // SAFETY: the outer handler initiates cleanup, whose second panic must terminate.
        let _=std::panic::catch_unwind(||unsafe{flux_double()});
        unreachable!();
    }
    for mode in 0..9{
        let before=ALLOCS.load(Ordering::Relaxed);
        let call=|value|{
            // SAFETY: the object implements these declarations and permits unwinding.
            unsafe{match mode{
                0=>flux_direct(value),1=>flux_indirect(raise_flux_probe,value),
                2=>flux_virtual(value),3=>flux_closure(value),
                4=>flux_aggregate(aggregate_probe,value),5=>flux_async(value),6=>flux_box(value),7=>flux_box_closure(value),_=>flux_box_destructor(value),
            }}
        };
        assert_eq!(call(3),6);
        assert_eq!(*EVENTS.lock().unwrap(),[1,13]);
        EVENTS.lock().unwrap().clear();
        let result=std::panic::catch_unwind(||{let _guard=Guard(99);call(-3)});
        assert_eq!(*result.unwrap_err().downcast::<String>().unwrap(),"owned payload");
        assert_eq!(*EVENTS.lock().unwrap(),[1,7,99]);
        EVENTS.lock().unwrap().clear();
        assert!(LIVE.lock().unwrap().is_empty(),"mode {mode}: leaked Flux allocation");
        if mode>=5 && std::env::var_os("CHECK_ALLOC").is_some(){
            assert!(ALLOCS.load(Ordering::Relaxed)>before,"mode {mode}: allocator probe was exercised");
        }
    }
    println!("payload and cleanup passed");
}
"#).unwrap();
    let reference = root.join("reference.rs");
    std::fs::write(&reference, SOURCE).unwrap();
    for (name, tier) in [
        ("rustc", None),
        ("none", Some(OptimizationTier::None)),
        ("full", Some(OptimizationTier::Full)),
    ] {
        let object = root.join(format!("{name}.o"));
        if let Some(tier) = tier {
            let mut module = module();
            let rename = |name: &mut String| {
                if matches!(
                    name.as_str(),
                    "malloc" | "calloc" | "realloc" | "free" | "posix_memalign"
                ) {
                    *name = format!("flux_{name}_probe");
                }
            };
            for declaration in &mut module.decls {
                rename(&mut declaration.name);
            }
            for function in &mut module.functions {
                let callees: Vec<_> = function.callees().map(|(id, _)| id).collect();
                for id in callees {
                    rename(&mut function.callee_mut(id).name);
                }
            }
            let bytes = flux_codegen::compile_object_with_panic_strategy(
                &mut module,
                Target::Aarch64AppleDarwin,
                tier,
                PanicStrategy::Unwind,
            )
            .unwrap();
            std::fs::write(&object, bytes).unwrap();
        } else {
            let output = std::process::Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--crate-type=lib",
                    "--emit=obj",
                    "-C",
                    "panic=unwind",
                ])
                .arg(&reference)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let binary = root.join(name);
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=unwind", "-l", "c++", "-C"])
            .arg(format!("link-arg={}", object.display()))
            .arg(&driver)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut command = std::process::Command::new(&binary);
        if tier.is_some() {
            command.env("CHECK_ALLOC", "1");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"payload and cleanup passed\n");
        let output = std::process::Command::new(&binary)
            .arg("boundary")
            .output()
            .unwrap();
        assert!(!output.status.success(), "C boundary must terminate");
        let stderr = String::from_utf8(output.stderr).unwrap();
        let events: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("cleanup "))
            .collect();
        assert_eq!(
            events,
            ["cleanup 7"],
            "{name}: boundary cleans its local before terminating: {stderr}"
        );
        let output = std::process::Command::new(&binary)
            .arg("double")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "a second escaping panic must terminate"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        let events: Vec<_> = stderr
            .lines()
            .filter(|line| line.starts_with("cleanup "))
            .collect();
        assert_eq!(
            events,
            ["cleanup 4", "cleanup 5"],
            "{name}: finish the whole value before terminating: {stderr}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
