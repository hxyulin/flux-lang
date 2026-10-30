//! Caller bounds retain their regions through compound self types and closures.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn caller_bound_environments_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-caller-bound-environments-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "into_forward",
            "fn need<'a,Q:?Sized,K>(_:&'a Q)where &'a Q:Into<K>{}fn f<'a,Q:?Sized,K>(x:&'a Q)where &'a Q:Into<K>{need::<Q,K>(x)}fn main(){}",
            true,
        ),
        (
            "into_method",
            "fn f<'a,Q:?Sized,K>(x:&'a Q)->K where &'a Q:Into<K>{x.into()}fn main(){}",
            true,
        ),
        (
            "closure_parent_bound",
            "trait Get<'a>{fn get(self)->&'a u32;}fn f<'a,T:Get<'a>>(x:T)->&'a u32{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            true,
        ),
        (
            "closure_trait_arg",
            "trait Get<T>{fn get(self)->T;}fn f<'a,T:Get<&'a u32>>(x:T)->&'a u32{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            true,
        ),
        (
            "into_static_mismatch",
            "fn need<K>(_:&'static str)->K where &'static str:Into<K>{panic!()}fn f<'a,K>(x:&'a str)->K where &'a str:Into<K>{need(x)}fn main(){}",
            false,
        ),
        (
            "into_wrong_bound",
            "fn need<'a,Q:?Sized,K>(_:&'a Q)where &'a Q:Into<K>{}fn f<'a,'b,Q:?Sized,K>(x:&'a Q)where &'b Q:Into<K>{need::<Q,K>(x)}fn main(){}",
            false,
        ),
        (
            "closure_parent_escape",
            "trait Get<'a>{fn get(self)->&'a u32;}fn f<'a,T:Get<'a>>(x:T)->&'static u32{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            false,
        ),
        (
            "closure_trait_arg_escape",
            "trait Get<T>{fn get(self)->T;}fn f<'a,T:Get<&'a u32>>(x:T)->&'static u32{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            false,
        ),
        (
            "closure_into_forward",
            "fn f<'a,Q:?Sized,K>(x:&'a Q)->K where &'a Q:Into<K>{Some(x).map(|x|x.into()).unwrap()}fn main(){}",
            true,
        ),
        (
            "nested_closure_parent",
            "trait Get<'a>{fn get(self)->&'a u32;}fn f<'a,T:Get<'a>>(x:T)->&'a u32{Some(x).map(|x|Some(x).map(|x|x.get()).unwrap()).unwrap()}fn main(){}",
            true,
        ),
        (
            "nested_closure_escape",
            "trait Get<'a>{fn get(self)->&'a u32;}fn f<'a,T:Get<'a>>(x:T)->&'static u32{Some(x).map(|x|Some(x).map(|x|x.get()).unwrap()).unwrap()}fn main(){}",
            false,
        ),
        (
            "closure_mutable_identity",
            "trait Get<T>{fn get(self)->T;}fn f<'a,T:Get<&'a mut u32>>(x:T)->&'a mut u32{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            true,
        ),
        (
            "closure_mutable_invariance",
            "trait Get<T>{fn get(self)->T;}fn f<'a,'b,T:Get<&'a mut &'a u32>>(x:T)->&'b mut &'b u32 where 'a:'b{Some(x).map(|x|x.get()).unwrap()}fn main(){}",
            false,
        ),
        (
            "owner_bound_closure",
            "trait Get<'a>{fn get(self)->&'a u32;}struct Wrap<T>(T);impl<'a,T:Get<'a>>Wrap<T>{fn run(self)->&'a u32{Some(self.0).map(|x|x.get()).unwrap()}}fn main(){}",
            true,
        ),
        (
            "owner_bound_closure_escape",
            "trait Get<'a>{fn get(self)->&'a u32;}struct Wrap<T>(T);impl<'a,T:Get<'a>>Wrap<T>{fn run(self)->&'static u32{Some(self.0).map(|x|x.get()).unwrap()}}fn main(){}",
            false,
        ),
        (
            "default_family_method",
            "trait Part<'d>{}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;fn part<'f>(&'f self)->Self::P<'f>;fn again(&self)->Self::P<'_>{self.part()}}fn main(){}",
            true,
        ),
        (
            "default_family_named",
            "trait Part<'d>{}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;fn part<'f>(&'f self)->Self::P<'f>;fn again<'f>(&'f self)->Self::P<'f>{self.part()}}fn main(){}",
            true,
        ),
        (
            "family_declared_bound",
            "trait Part<'d>{fn read(&self)->&'d u32;}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;fn part<'f>(&'f self)->Self::P<'f>;}fn read<'d,'f,T:Obj<'d>>(o:&'f T,p:&T::P<'f>)->&'d u32{p.read()}fn main(){}",
            true,
        ),
        (
            "default_family_escape",
            "trait Obj<'d>{type P<'f> where Self:'f,'d:'f;fn part<'f>(&'f self)->Self::P<'f>;fn bad(&self)->Self::P<'static>{self.part()}}fn main(){}",
            false,
        ),
        (
            "family_wrong_owner_lifetime",
            "trait Part<'d>{fn read(&self)->&'d u32;}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;}fn read<'d,'f,T:Obj<'d>>(p:&T::P<'f>)->&'static u32{p.read()}fn main(){}",
            false,
        ),
        (
            "family_input_implies_bound",
            "trait Obj<'d>{type P<'f> where Self:'f,'d:'f;}fn add<'d:'f,'f,T:Obj<'d>>(_:&'f T,_:&T::P<'f>){}fn new<'d,'f,T:Obj<'d>>(o:&'f T,p:&T::P<'f>){add(o,p)}fn main(){}",
            true,
        ),
        (
            "family_mutable_input_invariant",
            "trait Obj<'d>{type P<'f> where Self:'f,'d:'f;}fn wrong<'d,'a,'b,T:Obj<'d>>(x:&'a mut T::P<'a>)->&'b mut T::P<'b> where 'a:'b{x}fn main(){}",
            false,
        ),
        (
            "family_return_only_assumption",
            "trait G<'d>{type A<'a>where Self:'a,'d:'a;}fn f<'d,T:G<'d>>() -> T::A<'static>{panic!()}fn main(){}",
            true,
        ),
        (
            "family_return_only_call_rejects",
            "trait G<'d>{type A<'a>where Self:'a,'d:'a;}struct X<'d>(&'d u32);impl<'d>G<'d>for X<'d>{type A<'a>=&'d u32 where Self:'a,'d:'a;}fn f<'d,T:G<'d>>() -> T::A<'static>{panic!()}fn call<'d>(_:&'d u32){let _=f::<'d,X<'d>>();}fn main(){}",
            false,
        ),
        (
            "family_closure_output_bound",
            "trait Part<'d>{fn read(&self)->&'d u32;}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;type I<'f>:Iterator<Item=Self::P<'f>>where Self:'f,'d:'f;fn iter(&self)->Self::I<'_>;fn find(&self)->Option<Self::P<'_>>{self.iter().find(|p|*p.read()==7)}}fn main(){}",
            true,
        ),
        (
            "family_closure_output_escape",
            "trait Part<'d>{fn read(&self)->&'d u32;}trait Obj<'d>{type P<'f>:Part<'d> where Self:'f,'d:'f;type I<'f>:Iterator<Item=Self::P<'f>>where Self:'f,'d:'f;fn iter(&self)->Self::I<'_>;fn find(&self)->Option<Self::P<'static>>{self.iter().find(|p|*p.read()==7)}}fn main(){}",
            false,
        ),
        (
            "family_default_sized",
            "trait G{type A<'a>where Self:'a;}fn need<T>(){}fn f<'a,T:G+'a>(){need::<T::A<'a>>()}fn main(){}",
            true,
        ),
        (
            "family_slice_output_needs_relaxation",
            "trait G{type A<'a>where Self:'a;}struct S;impl G for S{type A<'a>=[u8];}fn main(){}",
            false,
        ),
        (
            "family_dyn_output_needs_relaxation",
            "trait G{type A<'a>where Self:'a;}struct S;impl G for S{type A<'a>=dyn core::fmt::Debug;}fn main(){}",
            false,
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
fn relaxed_family_output_matches_rustc() {
    let source = "trait G{type A<'a>:?Sized where Self:'a;}struct S;impl G for S{type A<'a>=[u8];}fn main(){}";
    let root = std::env::temp_dir().join(format!("flux-relaxed-family-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("relaxed.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
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
    std::fs::remove_dir_all(root).unwrap();
}
