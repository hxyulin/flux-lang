//! Explicit receiver signatures and anonymous trait imports retain ownership and scope.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn typed_receivers_and_anonymous_imports_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-typed-receivers-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "pinned_receiver_elision",
            "use core::pin::Pin;trait T{fn get(self:Pin<&mut Self>,other:&u32)->&u32;}struct S(u32);impl T for S{fn get(self:Pin<&mut Self>,_:&u32)->&u32{&self.get_mut().0}}fn main(){}",
            true,
        ),
        (
            "pinned_receiver_wrong_input",
            "use core::pin::Pin;trait T{fn get(self:Pin<&mut Self>,other:&u32)->&u32;}struct S(u32);impl T for S{fn get(self:Pin<&mut Self>,other:&u32)->&u32{other}}fn main(){}",
            false,
        ),
        (
            "pinned_receiver_escape",
            "use core::pin::Pin;struct S(u32);impl S{fn get(self:Pin<&mut Self>,_:&u32)->&u32{&self.get_mut().0}}fn main(){let other=7;let out;{let mut s=S(1);out=Pin::new(&mut s).get(&other);}assert_eq!(*out,1);}",
            false,
        ),
        (
            "shared",
            "struct S(u32);impl S{fn get(self:&Self)->u32{self.0}}fn main(){assert_eq!(S(3).get(),3);}",
            true,
        ),
        (
            "mutable",
            "struct S(u32);impl S{fn inc(self:&mut Self){self.0+=1}}fn main(){let mut x=S(3);x.inc();assert_eq!(x.0,4);}",
            true,
        ),
        (
            "value",
            "struct S(u32);impl S{fn get(self:Self)->u32{self.0}}fn main(){assert_eq!(S(3).get(),3);}",
            true,
        ),
        (
            "boxed",
            "struct S(String);impl S{fn take(self:Box<Self>)->String{self.0}}fn main(){assert_eq!(Box::new(S(String::from(\"x\"))).take(),\"x\");}",
            true,
        ),
        (
            "boxed_trait",
            "trait Take{fn take(self:Box<Self>)->u32;}struct S(u32);impl Take for S{fn take(self:Box<Self>)->u32{self.0}}fn main(){assert_eq!(Box::new(S(7)).take(),7);}",
            true,
        ),
        (
            "generic_boxed",
            "struct S<T>(T);impl<T> S<T>{fn take(self:Box<Self>)->T{self.0}}fn main(){assert_eq!(Box::new(S(7u32)).take(),7);}",
            true,
        ),
        (
            "typed_named_lifetime",
            "struct S(u32);impl S{fn get<'a>(self:&'a Self)->&'a u32{&self.0}}fn main(){let s=S(4);assert_eq!(*s.get(),4);}",
            true,
        ),
        (
            "mutable_binding",
            "struct S(u32);impl S{fn take(mut self:Box<Self>)->u32{self.0+=1;self.0}}fn main(){assert_eq!(Box::new(S(7)).take(),8);}",
            true,
        ),
        (
            "box_inherent_regression",
            "fn main(){let x=Box::new(7);let p=Box::into_raw(x);unsafe{drop(Box::from_raw(p));}}",
            true,
        ),
        (
            "unboxed_call",
            "struct S;impl S{fn take(self:Box<Self>){}}fn main(){S.take();}",
            false,
        ),
        (
            "moved_box",
            "struct S;impl S{fn take(self:Box<Self>){}}fn main(){let x=Box::new(S);x.take();x.take();}",
            false,
        ),
        (
            "shared_cannot_mutate",
            "struct S(u32);impl S{fn inc(self:&Self){self.0+=1}}fn main(){}",
            false,
        ),
        (
            "wrong_receiver",
            "struct S;impl S{fn bad(self:u32){}}fn main(){}",
            false,
        ),
        (
            "wrong_box_receiver",
            "struct S;impl S{fn bad(self:Box<u32>){}}fn main(){}",
            false,
        ),
        ("nonassociated", "fn bad(self:&Self){}fn main(){}", false),
        (
            "anonymous_trait",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}use m::T as _;fn main(){assert_eq!(7u32.get(),7);}",
            true,
        ),
        (
            "anonymous_duplicate",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}use m::T as _;use m::T as _;fn main(){assert_eq!(7u32.get(),7);}",
            true,
        ),
        (
            "anonymous_glob",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}mod p{pub use crate::m::T as _;}use p::*;fn main(){assert_eq!(7u32.get(),7);}",
            true,
        ),
        (
            "anonymous_block",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}fn main(){use m::T as _;assert_eq!(7u32.get(),7);}",
            true,
        ),
        (
            "anonymous_nontrait",
            "mod m{pub struct S;pub fn f(){}}use m::{S as _,f as _};fn main(){}",
            true,
        ),
        (
            "anonymous_not_named",
            "mod m{pub struct S;}use m::S as _;fn main(){let _=S;}",
            false,
        ),
        (
            "anonymous_missing",
            "use missing::T as _;fn main(){}",
            false,
        ),
        (
            "anonymous_private",
            "mod m{trait T{}}use m::T as _;fn main(){}",
            false,
        ),
        (
            "anonymous_trait_not_named",
            "mod m{pub trait T{fn get(&self)->u32;}}use m::T as _;fn f<T:m::T>(){} fn main(){let _: &dyn T;}",
            false,
        ),
        (
            "anonymous_sibling_scope",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}use m::T as _;mod sibling{pub fn f(){7u32.get();}}fn main(){}",
            false,
        ),
        (
            "boxed_autoref_wrong_self",
            "trait T{fn take(self:Box<Self>);}impl T for &u32{fn take(self:Box<Self>){}}fn main(){7u32.take();}",
            false,
        ),
        (
            "anonymous_self_module",
            "mod m{}use m::{self as _};fn main(){}",
            true,
        ),
        (
            "anonymous_self_trait",
            "trait T{}use T::{self as _};fn main(){}",
            true,
        ),
        (
            "anonymous_private_glob",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}mod a{use crate::m::T as _;pub mod b{pub use super::*;pub fn inside(){7u32.get();}}}use a::b::*;fn main(){7u32.get();}",
            false,
        ),
        (
            "anonymous_private_glob_inside",
            "mod m{pub trait T{fn get(&self)->u32;}impl T for u32{fn get(&self)->u32{*self}}}mod a{use crate::m::T as _;pub mod b{pub use super::*;pub fn inside(){7u32.get();}}}fn main(){a::b::inside();}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}

#[test]
fn boxed_dynamic_receivers_dispatch() {
    let source = "trait Take{fn take(self:Box<Self>)->u32;}struct S;impl Take for S{fn take(self:Box<Self>)->u32{7}}fn main(){let b:Box<dyn Take>=Box::new(S);assert_eq!(b.take(),7);}";
    let root = std::env::temp_dir().join(format!(
        "flux-boxed-dynamic-receiver-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("probe.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    // The vtable dispatches on the box, which the call moves, not on the
    // object behind it.
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
}

// Non-move closures copying a shared-reference field currently capture a
// borrow of its owner's field instead of Rust's independent shared pointer.
#[test]
#[ignore = "shared-reference field capture borrows its owner too broadly"]
fn shared_reference_field_capture_matches_rustc() {
    let source = "struct C<'a>{s:&'a[u32],n:u32}fn read(s:&[u32],n:u32)->bool{s.contains(&n)}impl C<'_>{fn select(&mut self,mut i:impl Iterator<Item=u32>){self.n=i.next().unwrap_or(0);}fn f(&mut self){let i=[1,2].into_iter().filter(|&x|read(self.s,x));self.select(i);}fn g(&mut self){let check=|x|read(self.s,x);self.select([1,2].into_iter().filter(|&x|check(x)));}}fn main(){let mut c=C{s:&[1,2],n:0};c.f();c.g();assert_eq!(c.n,1);}";
    let root =
        std::env::temp_dir().join(format!("flux-shared-field-capture-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("probe.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
}
