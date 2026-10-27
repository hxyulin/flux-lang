//! Nominal well-formedness retains trait region contracts at every represented use.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn adt_trait_contracts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-adt-trait-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "builtin_copy_callback",
            "struct Arena<T:Copy>(T);fn pass(p:fn()->Arena<&'static[u32]>)->fn()->Arena<&'static[u32]>{p}fn main(){}",
            true,
        ),
        (
            "conditional_copy_is_not_builtin",
            "#[derive(Clone)]struct Cond<'a>(&'a u32);impl<'a>Copy for Cond<'a>where 'a:'static{}struct Arena<T:Copy>(T);fn main(){let x=7;let _=Arena(Cond(&x));}",
            false,
        ),
        (
            "unrelated_argument_callback",
            "trait Alloc{}struct Global;impl Alloc for Global{}struct Holder<T,A:Alloc=Global>{value:T,allocator:A}fn pass<'a>(p:fn()->Holder<&'a u32>)->fn()->Holder<&'a u32>{p}fn main(){}",
            true,
        ),
        (
            "unrelated_argument_static",
            "use std::sync::{LazyLock,Mutex};trait Alloc{}struct Global;impl Alloc for Global{}struct Holder<T,A:Alloc=Global>{value:T,allocator:A}static DATA:u32=17;static LOCK:LazyLock<Mutex<Holder<&'static u32>>>=LazyLock::new(||Mutex::new(Holder{value:&DATA,allocator:Global}));fn main(){assert_eq!(*LOCK.lock().unwrap().value,17);}",
            true,
        ),
        (
            "alias_only",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}type A<'a>=C<'static,&'a u32>;fn main(){}",
            true,
        ),
        (
            "alias_use",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}type A<'a>=C<'static,&'a u32>;fn bad<'a>(_:Option<A<'a>>){}fn main(){}",
            false,
        ),
        (
            "argument_wf_missing",
            "use std::marker::PhantomData;trait Bound<T:'static>{}struct C<'a,R:Bound<&'a u32>>{r:R,a:PhantomData<&'a()>}fn main(){}",
            false,
        ),
        (
            "argument_wf_positive",
            "use std::marker::PhantomData;trait Bound<T:'static>{}struct C<'a:'static,R:Bound<&'a u32>>{r:R,a:PhantomData<&'a()>}fn main(){}",
            true,
        ),
        (
            "positive_field",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct Outer<'a,R:Read<'a>>{c:C<'a,R>}fn main(){}",
            true,
        ),
        (
            "positive_generic_construction",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn make<'a,R:Read<'a>>(r:R)->C<'a,R>{C{r,marker:PhantomData}}fn main(){}",
            true,
        ),
        (
            "positive_generic_signature",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn keep<'a,R:Read<'a>>(c:C<'a,R>)->C<'a,R>{c}fn main(){}",
            true,
        ),
        (
            "positive_impl_method",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}impl<'a,R:Read<'a>> C<'a,R>{fn get(&self)->&'a u32{self.r.read()}}fn main(){let x=7;let c=C{r:&x,marker:PhantomData};assert_eq!(*c.get(),7);}",
            true,
        ),
        (
            "unused_bad_field",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct Outer<'a,'b,R:Read<'b>>{c:C<'a,R>,marker:PhantomData<&'b()>}fn main(){}",
            false,
        ),
        (
            "unused_impl_header",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}impl<'a,'b,R:Read<'b>> C<'a,R>{fn unused(&self){}}fn main(){}",
            false,
        ),
        (
            "unused_nested_static_type",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn bad<'a,R:Read<'a>>(){let _:Option<C<'static,R>>=None;}fn main(){}",
            false,
        ),
        (
            "unused_static_construction",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn main(){let x=7;let c:C<'static,&u32>=C{r:&x,marker:PhantomData};drop(c);}",
            false,
        ),
        (
            "wrong_generic_construction",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn make<'a,'b,R:Read<'b>>(r:R)->C<'a,R>{C{r,marker:PhantomData}}fn main(){}",
            false,
        ),
        (
            "wrong_generic_signature",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn bad<'a,'b,R:Read<'b>>(c:C<'a,R>)->C<'a,R>{c}fn main(){}",
            false,
        ),
        (
            "wrong_generic_signature_outlives",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn bad<'a,'b,R:Read<'b>>(c:C<'a,R>)->C<'a,R>where 'b:'a{c}fn main(){}",
            false,
        ),
        (
            "shorten",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn f<'a:'b,'b,R:Read<'a>>(c:C<'a,R>)->C<'b,R>{c}fn main(){}",
            false,
        ),
        (
            "mutual",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn f<'a:'b,'b:'a,R:Read<'a>>(c:C<'a,R>)->C<'b,R>{c}fn main(){}",
            true,
        ),
        (
            "raw_signature",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn f<'a,'b,R:Read<'b>>(_: *const C<'a,R>){}fn main(){}",
            false,
        ),
        (
            "signature_no_body",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{fn bad<'a,'b,R:Read<'b>>(_:C<'a,R>);}fn main(){}",
            false,
        ),
        (
            "signature_no_body_positive",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{fn good<'a,R:Read<'a>>(_:C<'a,R>);}fn main(){}",
            true,
        ),
        (
            "size_of_argument",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn bad<'a>(x:&'a u32){fn take<T>(_:T){}take::<Option<C<'static,&u32>>>(Some(C{r:x,marker:PhantomData}));}fn main(){}",
            false,
        ),
        (
            "size_of_argument_positive",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}fn good(){let _=std::mem::size_of::<C<'static,&u32>>();}fn main(){}",
            true,
        ),
        (
            "enum_field",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}enum Outer<'a,'b,R:Read<'b>>{V(C<'a,R>),M(PhantomData<&'b()>)}fn main(){}",
            false,
        ),
        (
            "recursive_valid",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct Outer<'a,R:Read<'a>>{c:C<'a,R>,next:Option<Box<Outer<'a,R>>>}fn main(){}",
            true,
        ),
        (
            "local_argument_rows",
            "trait Read<T>{fn read(self,x:T)->T;}struct C<T,R:Read<T>>{r:R,t:std::marker::PhantomData<T>}fn good<'a,R:Read<&'a u32>>(r:R)->C<&'a u32,R>{C{r,t:std::marker::PhantomData}}fn main(){}",
            true,
        ),
        (
            "local_argument_wrong",
            "trait Read<T>{fn read(self,x:T)->T;}struct C<T,R:Read<T>>{r:R,t:std::marker::PhantomData<T>}fn bad<'a,'b,R:Read<&'a u32>>(r:R)->C<&'b u32,R>{C{r,t:std::marker::PhantomData}}fn main(){}",
            false,
        ),
        (
            "associated_value",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{type Out;}struct S<'a>(&'a u32);impl<'a>Api for S<'a>{type Out=C<'static,&'a u32>;}fn main(){}",
            false,
        ),
        (
            "associated_value_positive",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{type Out;}struct S<'a>(&'a u32);impl<'a>Api for S<'a>{type Out=C<'a,&'a u32>;}fn main(){}",
            true,
        ),
        (
            "bound_nested_type",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Mark<T>{}fn bad<'a,R:Mark<C<'static,&'a u32>>>(_:R){}fn main(){}",
            false,
        ),
        (
            "bound_nested_type_positive",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Mark<T>{}fn good<'a,R:Mark<C<'a,&'a u32>>>(_:R){}fn main(){}",
            true,
        ),
        (
            "family_value_positive",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{type Out<'a>;}struct S;impl Api for S{type Out<'a>=C<'a,&'a u32>;}fn main(){}",
            true,
        ),
        (
            "family_value_wrong",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}trait Api{type Out<'a>;}struct S;impl Api for S{type Out<'a>=C<'static,&'a u32>;}fn main(){}",
            false,
        ),
        (
            "duplicate_owner_rows",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,'b,R:Read<'a>+Read<'b>>{r:R,a:std::marker::PhantomData<(&'a(),&'b())>}fn main(){}",
            false,
        ),
        (
            "associated_constant_bad",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<'a>(&'a u32);impl<'a>S<'a>{const BAD:Option<C<'static,&'a u32>>=None;}fn main(){}",
            false,
        ),
        (
            "associated_constant_good",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<'a>(&'a u32);impl<'a>S<'a>{const GOOD:Option<C<'a,&'a u32>>=None;}fn main(){}",
            true,
        ),
        (
            "associated_constant_generic_bad",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const BAD:Option<C<'static,R>>=None;}fn main(){}",
            false,
        ),
        (
            "associated_constant_generic_good",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const GOOD:Option<C<'a,R>>=None;}fn main(){}",
            true,
        ),
        (
            "associated_constant_local_bad",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const BAD:usize={let _:PhantomData<Option<C<'static,R>>>=PhantomData;0};}fn main(){}",
            false,
        ),
        (
            "associated_constant_local_good",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const GOOD:usize={let _:PhantomData<Option<C<'_,R>>>=PhantomData;0};}fn main(){}",
            true,
        ),
        (
            "associated_constant_typearg_bad",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const BAD:usize=std::mem::size_of::<Option<C<'static,R>>>();}fn main(){}",
            false,
        ),
        (
            "associated_constant_typearg_good",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}struct S<R>(R);impl<'a,R:Read<'a>>S<R>{const GOOD:usize=std::mem::size_of::<Option<C<'_,R>>>();}fn main(){}",
            true,
        ),
        (
            "static_no_owner",
            "use std::marker::PhantomData;trait Read<'a>{fn read(&self)->&'a u32;}impl<'a>Read<'a>for &'a u32{fn read(&self)->&'a u32{*self}}struct C<'a,R:Read<'a>>{r:R,marker:PhantomData<&'a()>}static VALUE:Option<C<'static,&'static u32>>=None;fn main(){}",
            true,
        ),
        (
            "fold_projected_callback",
            "trait Alloc{}impl Alloc for(){}struct Drain<'a,T,A:Alloc>{value:&'a T,alloc:A}impl<'a,T,A:Alloc>Iterator for Drain<'a,T,A>{type Item=&'a T;fn next(&mut self)->Option<Self::Item>{None}fn fold<B,F>(self,init:B,_:F)->B where Self:Sized,F:FnMut(B,Self::Item)->B{init}}fn main(){}",
            true,
        ),
        (
            "owner_self_sized_contract",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}impl<'a,R:Read<'a>>C<'a,R>{fn consume(self)where Self:Sized{}}fn main(){}",
            true,
        ),
        (
            "owner_self_static_return",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}impl<'a,R:Read<'a>>C<'a,R>{fn bad(self)->C<'static,R>where Self:Sized{loop{}}}fn main(){}",
            false,
        ),
        (
            "owner_self_explicit_static",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}impl<'a,R:Read<'a>>C<'a,R>{fn bad(self)where C<'static,R>:Sized{}}fn main(){}",
            false,
        ),
        (
            "owner_self_alias_static",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}type Alias<'x,R>=C<'x,R>;impl<'a,R:Read<'a>>C<'a,R>{fn bad(self)where Alias<'static,R>:Sized{}}fn main(){}",
            false,
        ),
        (
            "owner_self_wrong_function_lifetime",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}impl<'a,R:Read<'a>>C<'a,R>{fn bad<'b>(self,_:C<'b,R>)where Self:Sized{}}fn main(){}",
            false,
        ),
        (
            "owner_other_impl_contract",
            "trait Read<'a>{fn read(&self)->&'a u32;}struct C<'a,R:Read<'a>>{reader:R,marker:std::marker::PhantomData<&'a()>}struct Other<R>(R);impl<R:Read<'static>>Other<R>{fn good(self)->C<'static,R>{C{reader:self.0,marker:std::marker::PhantomData}}}impl<'a,R:Read<'a>>C<'a,R>{fn bad(self)->C<'static,R>where Self:Sized{loop{}}}fn main(){}",
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
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if name != "argument_wf_missing" && name != "duplicate_owner_rows" {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: {:?}",
                typed.diagnostics
            );
        }
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            typed.diagnostics.is_empty() && mir.diagnostics.is_empty(),
            accepted,
            "{name}: type {:?}; MIR {:?}",
            typed.diagnostics,
            mir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
