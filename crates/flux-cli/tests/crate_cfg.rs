//! Explicit source-crate cfg inputs agree with rustc and stay within their crate.
use std::path::PathBuf;
use std::process::{Command,Output};
use std::sync::atomic::{AtomicU32,Ordering};
use flux_hir::{CrateCfg,SourceCrate,SourceDb};
use flux_intern::Symbol;

struct Case(PathBuf);
impl Case {
    fn new()->Self{static NEXT:AtomicU32=AtomicU32::new(0);let root=std::env::temp_dir().join(format!("flux-crate-cfg-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir_all(&root).unwrap();Self(root)}
    fn write(&self,name:&str,text:&str)->PathBuf{let path=self.0.join(name);std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(&path,text).unwrap();path}
}
impl Drop for Case{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn config(feature:&str,tune:bool,flavor:&str)->CrateCfg{CrateCfg{names:if tune{vec![Symbol::intern("tune")]}else{Vec::new()},values:vec![(Symbol::intern("feature"),Symbol::intern(feature)),(Symbol::intern("flavor"),Symbol::intern(flavor))]}}
fn apply(command:&mut Command,cfg:&CrateCfg){for name in &cfg.names{command.arg("--cfg").arg(name.as_str());}for(name,value)in &cfg.values{command.arg("--cfg").arg(format!("{}={:?}",name,value.as_str()));}}
fn success(output:Output)->Output{assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));output}

#[test]
fn disjoint_features_shared_files_and_macro_boundaries_match_rustc(){
    let case=Case::new();
    let shared=r#"
#[cfg_attr(feature="alpha",cfg_attr(tune,derive(Clone,Copy)))]
pub struct Marker(pub u32);
pub fn answer(#[cfg(feature="alpha")]x:u32,#[cfg(not(feature="alpha"))]y:u32)->u32{
    #[cfg(feature="alpha")] {x+1}
    #[cfg(not(feature="alpha"))] {y+2}
}
pub fn flags()->(bool,bool,bool,bool){(cfg!(feature="alpha"),cfg!(feature="beta"),cfg!(tune),cfg!(flavor="fast"))}
"#;
    case.write("shared/module.rs",shared);
    case.write("shared/included.rs",r#"
#[cfg(feature="beta")] fn included()->u32{29}
#[cfg(not(feature="beta"))] fn included()->u32{31}
"#);
    let a=case.write("a/lib.rs",r#"
#[path="../shared/module.rs"]pub mod shared;
#[cfg(feature="alpha")]#[macro_export]macro_rules! selected{()=>{23};}
#[cfg(not(feature="alpha"))]#[macro_export]macro_rules! selected{()=>{99};}
#[macro_export]macro_rules! call_cfg{()=>{(cfg!(feature="alpha"),cfg!(feature="beta"),cfg!(tune),cfg!(flavor="fast"))};}
#[macro_export]macro_rules! declare{()=>{#[cfg(feature="beta")]fn emitted()->u32{37}#[cfg(not(feature="beta"))]fn emitted()->u32{41}};}
#[macro_export]macro_rules! included_source{()=>{include!("shared/included.rs");};}
"#);
    let b=case.write("b/lib.rs","#[path=\"../shared/module.rs\"]pub mod shared;");
    let main=case.write("main.rs",r#"
a::declare!();a::included_source!();
fn main(){let marker=a::shared::Marker(7);let copy=marker;println!("{} {} {} {} {} {}",a::shared::answer(10),b::shared::answer(10),marker.0+copy.0,a::selected!(),emitted(),included());println!("{:?} {:?} {:?}",a::shared::flags(),b::shared::flags(),a::call_cfg!());}
"#);
    let a_cfg=config("alpha",true,"fast");let b_cfg=config("gamma",false,"slow");let main_cfg=config("beta",false,"slow");
    let mut db=SourceDb::new();let ar=db.load(&a).unwrap();let br=db.load(&b).unwrap();let mr=db.load(&main).unwrap();
    db.set_crate_cfg(ar,a_cfg.clone()).unwrap();db.set_crate_cfg(br,b_cfg.clone()).unwrap();db.set_crate_cfg(mr,main_cfg.clone()).unwrap();
    let hir=flux_hir::lower_sources(&mut db,&[
        SourceCrate{root:ar,name:Symbol::intern("a"),dependencies:vec![]},
        SourceCrate{root:br,name:Symbol::intern("b"),dependencies:vec![]},
        SourceCrate{root:mr,name:Symbol::intern("main"),dependencies:vec![(Symbol::intern("a"),0),(Symbol::intern("b"),1)]},
    ]);
    assert!(db.syntax_errors().is_empty(),"{:?}",db.syntax_errors());assert!(hir.diagnostics.is_empty(),"{:?}",hir.diagnostics);
    assert!(hir.items.crates.iter().filter(|c|c.is_core||c.name.as_str()=="std").all(|c|c.cfg.is_none()));
    assert_eq!(std::fs::read_to_string(case.0.join("shared/module.rs")).unwrap(),shared);
    let typed=flux_mir::check_file(&hir);assert!(typed.diagnostics.is_empty(),"{:?}",typed.diagnostics);
    let mir=flux_mir::build_file(&hir,&typed);assert!(mir.diagnostics.is_empty(),"{:?}",mir.diagnostics);
    for(name,path,cfg)in[("a",&a,&a_cfg),("b",&b,&b_cfg)]{let mut command=Command::new("rustc");command.args(["--edition=2024","--crate-type=rlib","-Cpanic=abort","--crate-name"]).arg(name).arg(path).arg("-o").arg(case.0.join(format!("lib{name}.rlib")));apply(&mut command,cfg);success(command.output().unwrap());}
    let executable=case.0.join("rustc");let mut command=Command::new("rustc");command.args(["--edition=2024","-Cpanic=abort"]).arg(&main).arg("-o").arg(&executable);apply(&mut command,&main_cfg);for name in ["a","b"]{command.arg("--extern").arg(format!("{name}={}",case.0.join(format!("lib{name}.rlib")).display()));}success(command.output().unwrap());
    #[cfg(all(target_os="macos",target_arch="aarch64"))]{
        let expected=success(Command::new(&executable).output().unwrap());
        assert_eq!(expected.stdout,b"11 12 14 23 37 29\n(true, false, true, true) (false, false, false, false) (false, true, false, false)\n");
        for tier in [flux_codegen::OptimizationTier::None,flux_codegen::OptimizationTier::Full]{let mut module=flux_codegen::lower_module(&hir,&typed,&mir).unwrap();let target=flux_codegen::Target::Aarch64AppleDarwin;let object=flux_codegen::compile_object(&mut module,target,tier).unwrap();let executable=case.0.join("flux");flux_codegen::link_executable(&object,target,&executable).unwrap();let actual=success(Command::new(executable).output().unwrap());assert_eq!(actual.stdout,expected.stdout);assert_eq!(actual.stderr,expected.stderr);}
    }
}

#[test]
fn root_configs_do_not_leak_between_compilations_or_default_crates(){
    let case=Case::new();let path=case.write("main.rs","#[cfg(feature=\"std\")]fn value()->u32{7}#[cfg(not(feature=\"std\"))]fn value()->u32{9}fn main(){println!(\"{} {}\",value(),cfg!(feature=\"std\"));}");
    for enabled in [true,false,true]{let mut db=SourceDb::new();let root=db.load(&path).unwrap();if enabled{db.set_crate_cfg(root,CrateCfg{names:Vec::new(),values:vec![(Symbol::intern("feature"),Symbol::intern("std"))]}).unwrap();}let hir=flux_hir::lower(&mut db,root,Symbol::intern("main"));assert!(hir.diagnostics.is_empty(),"{:?}",hir.diagnostics);let (_,body)=hir.bodies.iter().find(|(id,_)|hir.items.functions[*id].name.as_str()=="value").unwrap();let ints:Vec<_>=body.0.exprs.iter().filter_map(|(_,expr)|if let flux_hir::Expr::Literal(flux_hir::Literal::Int{value,..})=expr{Some(*value)}else{None}).collect();assert_eq!(ints,[if enabled{7}else{9}]);}
}
