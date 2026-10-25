//! IEEE floating-point language operations match Rust.
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn floats_match_rustc_off_and_on() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-floats-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["floats", "float_decimal", "float_patterns"] {
    let source = root.join(format!("tests/run/{name}.rs"));
    let reference = dir.join("rustc");
    let output = Command::new("rustc").args(["--edition=2024", "-C", "panic=abort", "-C", "overflow-checks=off"]).arg(&source).arg("-o").arg(&reference).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr).lines().take(20).collect::<Vec<_>>().join("\n"));
    let expected = Command::new(&reference).output().unwrap();
    assert!(expected.status.success());
    for optimize in [false, true] {
        let executable = dir.join(if optimize { "flux-opt" } else { "flux" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimize { command.arg("-O"); }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr).lines().take(20).collect::<Vec<_>>().join("\n"));
        let actual = Command::new(&executable).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code());
        if actual.stdout != expected.stdout {
            let a=String::from_utf8_lossy(&actual.stdout); let e=String::from_utf8_lossy(&expected.stdout);
            let mismatch=a.lines().zip(e.lines()).position(|(a,e)|a!=e);
            panic!("{name}, optimize={optimize}, first differing line {mismatch:?}: actual {:?}, expected {:?}; artifacts {}", mismatch.and_then(|i|a.lines().nth(i)), mismatch.and_then(|i|e.lines().nth(i)), dir.display());
        }
        assert_eq!(actual.stderr, expected.stderr);
    }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn float_acceptance_matches_rustc() {
    let dir=std::env::temp_dir().join(format!("flux-float-oracle-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name,code,accepts) in [
        ("literal_suffix","fn f()->f32{1f32}",true),
        ("float_nonexhaustive_f32","fn f(x:f32)->u8{match x{1.0=>1}}",false),
        ("float_nonexhaustive_f64","fn f(x:f64)->u8{match x{-0.0=>1,1.0=>2}}",false),
        ("float_refutable_let","fn f(){let 1.0f32=2.0f32;}",false),
        ("float_refutable_negative_zero","fn f(){let -0.0f64=0.0f64;}",false),
        ("float_refutable_argument","fn f(1.0:f32){}",false),
        ("float_refutable_tuple","fn f(){let (1.0,_)=(2.0f64,true);}",false),
        ("float_refutable_binding","fn f(){let x@1.0=2.0f32;}",false),
        ("float_zero_or_refutable","fn f(x:f64)->u8{match x{0.0|-0.0=>1}}",false),
        ("float_nested_nonexhaustive","fn f(x:Option<f32>){match x{Some(1.0)=>(),None=>()}}",false),
        ("float_wildcard","fn f(x:f32)->u8{match x{1.0=>1,_=>2}}",true),
        ("float_nested_wildcard","fn f(x:(f64,bool))->u8{match x{(0.0,_)=>1,(_,true)=>2,(_,false)=>3}}",true),
        ("float_let_else","fn f(x:f32)->u8{let 1.0=x else{return 0;};1}",true),
        ("float_reference_wildcard","fn f(x:&f64)->u8{match x{1.5=>1,_=>2}}",true),
        ("float_duplicate_zero","fn f(x:f64)->u8{match x{0.0=>1,-0.0=>2,_=>3}}",true),
        ("float_nan_pattern","const N:f32=f32::NAN;fn f(x:f32){match x{N=>(),_=>()}}",false),
        ("literal_context","fn f()->f32{1.0000000596046447753906250001}",true),
        ("default_f64","fn f(){let x=1.0;let _:f64=x;}",true),
        ("mixed_width","fn f(){let _=1f32+2f64;}",false),
        ("int_float","fn f(){let _=1+2.0;}",false),
        ("float_integer_annotation","fn f(){let _:u8=1.0;}",false),
        ("bitwise","fn f(){let _=1.0f64&2.0;}",false),
        ("shift","fn f(){let _=1.0f64<<2;}",false),
        ("not","fn f(){let _= !1.0f32;}",false),
        ("bool_cast","fn f(){let _=true as f32;}",false),
        ("char_cast","fn f(){let _='a' as f64;}",false),
        ("to_bool","fn f(){let _=1.0 as bool;}",false),
        ("pointer_cast","fn f(p:*const u8){let _=p as f64;}",false),
        ("to_pointer","fn f(){let _=1.0 as *const u8;}",false),
        ("const_generic","fn f<const N:f32>(){}",false),
        ("overflow_f32","fn f(){let _=1e100f32;}",false),
        ("overflow_f64","fn f(){let _=1e999;}",false),
        ("nan_const","const N:f64=0.0/0.0;fn f(){let _=N;}",true),
        ("const_casts","const A:i128=1e38f64 as i128;const B:f32=u128::MAX as f32;",true),
        ("bits_const","const A:u32=f32::from_bits(0x7fc01234).to_bits();",true),
        ("no_eq","fn need<T:Eq>(){}fn f(){need::<f32>();}",false),
        ("send_sync","fn need<T:Send+Sync>(){}fn f(){need::<f32>();need::<f64>();}",true),
        ("function_pointer_width","fn f(_:f32){}fn g(){let _:fn(f64)=f;}",false),
        ("variadic_promotion","unsafe extern \"C\"{fn printf(p:*const i8,...);}fn f(){unsafe{printf(core::ptr::null(),1.0f32);}}",false),
    ] {
        let source=dir.join(format!("{name}.rs"));
        std::fs::write(&source,format!("{code}\nfn main(){{}}")).unwrap();
        let rustc=Command::new("rustc").args(["--edition=2024","--emit=metadata","-o"]).arg(dir.join(format!("{name}.rmeta"))).arg(&source).output().unwrap();
        assert_eq!(rustc.status.success(),accepts,"rustc {name}: {}",String::from_utf8_lossy(&rustc.stderr));
        let flux=Command::new(env!("CARGO_BIN_EXE_flux")).arg("mir").arg(&source).output().unwrap();
        assert_eq!(flux.status.success(),accepts,"Flux {name}: {}",String::from_utf8_lossy(&flux.stderr));
        assert!(!String::from_utf8_lossy(&flux.stderr).contains("panicked"),"{name}: {}",String::from_utf8_lossy(&flux.stderr));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_arch="aarch64",target_os="macos"))]
fn native_c_float_aggregates_and_callbacks() {
    let dir=std::env::temp_dir().join(format!("flux-float-c-{}",std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let native=dir.join("native.rs");
    std::fs::write(&native,r#"
#[repr(C)]#[derive(Clone,Copy)]pub struct Pair{a:f32,b:f32}
#[repr(C)]#[derive(Clone,Copy)]pub struct Mixed{tag:u8,a:f64,b:f32}
#[unsafe(no_mangle)]pub extern "C" fn pair(mut x:Pair)->Pair{x.a+=1.25;x.b*=2.0;x}
#[unsafe(no_mangle)]pub extern "C" fn mixed(mut x:Mixed)->Mixed{x.tag+=1;x.a+=2.5;x.b-=1.25;x}
#[unsafe(no_mangle)]pub extern "C" fn callback(f:extern "C" fn(Pair)->Pair)->f64{let x=f(Pair{a:1.5,b:2.25});x.a as f64+x.b as f64}
"#).unwrap();
    let library=dir.join("native.dylib");
    let output=Command::new("rustc").args(["--edition=2024","--crate-type=cdylib","-C","panic=abort"]).arg(&native).arg("-o").arg(&library).output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let source=dir.join("consumer.rs");
    let code=r#"
#[repr(C)]#[derive(Clone,Copy)]struct Pair{a:f32,b:f32}
#[repr(C)]#[derive(Clone,Copy)]struct Mixed{tag:u8,a:f64,b:f32}
unsafe extern "C"{fn dlopen(p:*const i8,m:i32)->*mut u8;fn dlsym(h:*mut u8,s:*const i8)->*mut u8;}
extern "C" fn local(mut x:Pair)->Pair{x.a+=3.0;x.b+=4.0;x}
fn main(){unsafe{
let h=dlopen(c"__LIBRARY__".as_ptr(),2);assert!(!h.is_null());
let pair:unsafe extern "C" fn(Pair)->Pair=core::mem::transmute(dlsym(h,c"pair".as_ptr()));
let mixed:unsafe extern "C" fn(Mixed)->Mixed=core::mem::transmute(dlsym(h,c"mixed".as_ptr()));
let callback:unsafe extern "C" fn(extern "C" fn(Pair)->Pair)->f64=core::mem::transmute(dlsym(h,c"callback".as_ptr()));
let original=Pair{a:1.5,b:2.25};let x=pair(original);assert_eq!(original.a.to_bits(),1.5f32.to_bits());
assert_eq!(x.a.to_bits(),2.75f32.to_bits());assert_eq!(x.b.to_bits(),4.5f32.to_bits());
let y=mixed(Mixed{tag:7,a:8.5,b:3.0});assert_eq!(y.tag,8);assert_eq!(y.a.to_bits(),11.0f64.to_bits());assert_eq!(y.b.to_bits(),1.75f32.to_bits());
assert_eq!(callback(local).to_bits(),10.75f64.to_bits());println!("{} {}",x.a.to_bits(),y.a.to_bits());
}}
"#;
    std::fs::write(&source,code.replace("__LIBRARY__",library.to_str().unwrap())).unwrap();
    let reference=dir.join("reference");
    let output=Command::new("rustc").args(["--edition=2024","-C","panic=abort"]).arg(&source).arg("-o").arg(&reference).output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let expected=Command::new(&reference).output().unwrap();assert!(expected.status.success(),"{}",String::from_utf8_lossy(&expected.stderr));
    for optimize in [false,true] {
        let executable=dir.join(format!("flux-{optimize}"));let mut command=Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);if optimize{command.arg("-O");}
        let output=command.output().unwrap();assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr).lines().take(20).collect::<Vec<_>>().join("\n"));
        let actual=Command::new(&executable).output().unwrap();assert_eq!(actual.status.code(),expected.status.code(),"{}",String::from_utf8_lossy(&actual.stderr));assert_eq!(actual.stdout,expected.stdout);assert_eq!(actual.stderr,expected.stderr);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
