//! Named closure-parameter lifetimes retain their signature regions.
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    let stdout = directory.join(format!("{label}.stdout"));
    let stderr = directory.join(format!("{label}.stderr"));
    let mut child = command.stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap())).spawn().unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap(); child.wait().unwrap(); panic!("{label} exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output { status, stdout: std::fs::read(stdout).unwrap(), stderr: std::fs::read(stderr).unwrap() }
}



#[test]
fn named_closure_parameters_match_rustc() {
    let root=std::env::temp_dir().join(format!("flux-closure-annotations-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name,source,accepts) in [
        ("outer", "fn f<'a>(x:&'a str)->&'a str{let identity=|value:&'a str|value;identity(x)}fn main(){}",true),
        ("outer_wrong", "fn f<'a,'b>(x:&'a str,_y:&'b str)->&'b str{let identity=|value:&'b str|value;identity(x)}fn main(){}",false),
        ("outer_bound", "fn f<'a:'b,'b>(x:&'a str,_y:&'b str)->&'b str{let identity=|value:&'b str|value;identity(x)}fn main(){}",true),
        ("outer_local", "fn f<'a>(_:&'a str){let identity=|value:&'a str|value;let local=String::from(\"short\");identity(&local);}fn main(){}",false),
        ("static", "fn main(){let count=|how:&'static str|how.len();assert_eq!(count(\"static\"),6);}",true),
        ("static_short", "fn main(){let count=|how:&'static str|how.len();let s=String::from(\"short\");count(&s);}",false),
        ("nested_static", "fn main(){let first=|input:Vec<(&'static str,u32)>|input[0].1;assert_eq!(first(vec![(\"key\",7)]),7);}",true),
        ("nested_static_short", "fn main(){let first=|input:Vec<(&'static str,u32)>|input[0].1;let s=String::from(\"short\");first(vec![(s.as_str(),7)]);}",false),
        ("static_not_polymorphic", "fn apply<F:Fn(&str)>(f:F){let s=String::from(\"short\");f(&s);}fn main(){apply(|_:&'static str|{});}",false),
        ("anonymous_escape", "fn main(){let mut out=None;let mut f=|x:&str|out=Some(x);f(\"text\");}",false),
        ("dyn_adt", "struct Node<'a>{text:&'a str}fn apply(f:&mut dyn FnMut(Node<'_>,&mut Vec<String>)->usize){let text=String::from(\"node\");let mut out=Vec::new();assert_eq!(f(Node{text:&text},&mut out),4);}fn main(){apply(&mut|node,out|{out.push(node.text.into());node.text.len()});}",true),
        ("dyn_mut_adt", "struct Ctx<'a>{text:&'a str,count:usize}fn apply(f:&mut dyn FnMut(&mut Ctx<'_>,usize)->usize){let text=String::from(\"node\");let mut ctx=Ctx{text:&text,count:0};assert_eq!(f(&mut ctx,3),7);}fn main(){apply(&mut|ctx,n|{ctx.count+=n;ctx.text.len()+ctx.count});}",true),
        ("dyn_adt_escape", "struct Node<'a>{text:&'a str}fn main(){let mut saved=None;let _f:&mut dyn FnMut(Node<'_>)=&mut|node|saved=Some(node.text);}",false),
        ("dyn_nested_escape", "struct Ctx<'a>{text:&'a str}fn main(){let mut saved=None;let _f:&mut dyn FnMut(&mut Ctx<'_>)=&mut|ctx|saved=Some(ctx.text);}",false),
        ("dyn_independent_inputs", "fn main(){let _f:&mut dyn FnMut(&mut &str,&str)=&mut|dest,src|*dest=src;}",false),
        ("hrtb_elided_output", "fn apply<F:for<'a>Fn(&'a u32)->&u32>(f:F)->u32{let x=7;*f(&x)}fn main(){assert_eq!(apply(|x|x),7);}",true),
        ("hrtb_distinct", "fn apply<F:for<'a,'b>Fn(&'a u32,&'b u32)->&'a u32>(f:F)->u32{let x=7;let y=9;*f(&x,&y)}fn main(){assert_eq!(apply(|x,_|x),7);}",true),
        ("hrtb_unbound_output", "fn apply<F:for<'a>Fn()->&'a u32>(_:F){}fn main(){}",false),
    ] {
        let path=root.join(format!("{name}.rs"));std::fs::write(&path,source).unwrap();
        let expected=bounded(Command::new("rustc").args(["--edition=2024","--emit=metadata","-o"]).arg(root.join(format!("{name}.rmeta"))).arg(&path),&root,&format!("rustc-{name}"));
        assert_eq!(expected.status.success(),accepts,"rustc {name}: {}",String::from_utf8_lossy(&expected.stderr));
        let actual=bounded(Command::new(env!("CARGO_BIN_EXE_flux")).arg("mir").arg(&path),&root,&format!("flux-{name}"));
        assert_eq!(actual.status.success(),accepts,"flux {name}: {}",String::from_utf8_lossy(&actual.stderr));
    }
}

#[test]
fn callable_alias_regions_keep_an_explicit_metadata_boundary() {
    let root=std::env::temp_dir().join(format!("flux-callable-alias-regions-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name,source) in [
        ("static_alias", "type Input=&'static str;fn apply(_: &dyn Fn(Input)){}fn main(){}"),
        ("repeated_alias", "type Input<'a>=(&'a str,&'a str);fn apply(_: &dyn Fn(Input<'_>)){}fn main(){}"),
    ] {
        let path=root.join(format!("{name}.rs"));std::fs::write(&path,source).unwrap();
        let expected=bounded(Command::new("rustc").args(["--edition=2024","--emit=metadata","-o"]).arg(root.join(format!("{name}.rmeta"))).arg(&path),&root,&format!("rustc-{name}"));
        assert!(expected.status.success(),"{}",String::from_utf8_lossy(&expected.stderr));
        let actual=bounded(Command::new(env!("CARGO_BIN_EXE_flux")).arg("mir").arg(&path),&root,&format!("flux-{name}"));
        assert!(!actual.status.success());
        assert!(String::from_utf8_lossy(&actual.stderr).contains("references and trait objects in trait objects' arguments are not supported yet"),"{}",String::from_utf8_lossy(&actual.stderr));
    }
}

#[test]
#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn named_closure_annotations_execute_like_rustc() {
    let root=std::env::temp_dir().join(format!("flux-closure-annotations-native-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/run/closure_named_lifetimes.rs");
    let reference=root.join("rustc");
    let output=bounded(Command::new("rustc").args(["--edition=2024","-Cpanic=abort","-o"]).arg(&reference).arg(&source),&root,"compile-rustc");
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let expected=bounded(&mut Command::new(reference),&root,"run-rustc");assert!(expected.status.success());
    assert_eq!(expected.stdout,b"borrowed static 7\n");
    for optimized in [false,true] {
        let executable=root.join(format!("flux-{optimized}"));let mut command=Command::new(env!("CARGO_BIN_EXE_flux"));command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {command.arg("-O");}
        let output=bounded(&mut command,&root,&format!("compile-flux-{optimized}"));
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let actual=bounded(&mut Command::new(executable),&root,&format!("run-flux-{optimized}"));
        assert_eq!(actual.status.code(),expected.status.code());assert_eq!(actual.stdout,expected.stdout);assert_eq!(actual.stderr,expected.stderr);
    }
}
