//! Non-callable object argument rows retain independent associated lifetimes.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn errors(name: &str, source: &str) -> Vec<String> {
    let mut db = SourceDb::new();
    let file = db.add_with_edition(
        format!("/object-arguments/{name}.rs"),
        source,
        Edition::E2021,
    );
    let hir = flux_hir::lower_sources(
        &mut db,
        &[SourceCrate {
            root: file,
            name: Symbol::intern(name),
            dependencies: vec![],
        }],
    );
    let typed = flux_ty::check_file(&hir);
    let mut errors: Vec<_> = hir
        .diagnostics
        .iter()
        .chain(&typed.diagnostics)
        .map(|d| d.message.clone())
        .collect();
    if errors.is_empty() {
        errors.extend(
            flux_mir::build_file(&hir, &typed)
                .diagnostics
                .into_iter()
                .map(|d| d.message),
        );
    }
    errors
}

#[test]
fn object_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-object-arguments-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "call_independent_same",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a,'b>(x:&(dyn Get<(&'a u32,&'b u32)>+'static)){need::<dyn Get<(&'a u32,&'b u32)>,(&'a u32,&'b u32)>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_independent_swapped",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a,'b>(x:&(dyn Get<(&'a u32,&'b u32)>+'static)){need::<dyn Get<(&'a u32,&'b u32)>,(&'b u32,&'a u32)>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_user_marker_changed",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+Send+'static)){need::<dyn Get<&'a u32>+Send,&'a u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_user_associated_same",
            r###"trait Get<T>{type Item;}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T,Item=u32>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32,Item=u32>+'static)){need::<dyn Get<&'a u32,Item=u32>,&'a u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_user_associated_changed",
            r###"trait Get<T>{type Item;}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T,Item=u32>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32,Item=u64>+'static)){need::<dyn Get<&'a u32,Item=u64>,&'a u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_explicit_marker_impl",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>+Send+Sync{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+Sync+Send+'static)){need::<dyn Get<&'a u32>+Send+Sync,&'a u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_marker_send",
            r###"trait Get<T>{}fn need<T:Send+?Sized>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+Send+'static)){need(x)}fn main(){}"###,
            true,
        ),
        (
            "call_marker_sync",
            r###"trait Get<T>{}fn need<T:Sync+?Sized>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+Sync+'static)){need(x)}fn main(){}"###,
            true,
        ),
        (
            "call_super_marker",
            r###"trait Get<T>:Send{}fn need<T:Send+?Sized>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need(x)}fn main(){}"###,
            true,
        ),
        (
            "call_membership_static_required",
            r###"trait Foo<T>:std::any::Any{}fn need<T:Foo<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Foo<&'a u32>+'static)){need::<dyn Foo<&'a u32>,&'a u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_membership_static_ok",
            r###"trait Foo<T>:std::any::Any{}fn need<T:Foo<U>+?Sized,U>(_:&T){}fn f(x:&(dyn Foo<&'static u32>+'static)){need::<dyn Foo<&'static u32>,&'static u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_user_impl_condition",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T:'static>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'a u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_principal_same",
            r###"trait Get<T>{}fn need<T:Get<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'a u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_principal_wrong_static",
            r###"trait Get<T>{}fn need<T:Get<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'static u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_super_same",
            r###"trait Base<T>{}trait Get<T>:Base<T>{}fn need<T:Base<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'a u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_super_wrong_static",
            r###"trait Base<T>{}trait Get<T>:Base<T>{}fn need<T:Base<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'static u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_user_impl_same",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'a u32>(x)}fn main(){}"###,
            true,
        ),
        (
            "call_user_impl_wrong_static",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,&'static u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_inferred_self_wrong",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<_,&'static u32>(x)}fn main(){}"###,
            false,
        ),
        (
            "call_inferred_arg",
            r###"trait Get<T>{}trait Raw<T>:Get<T>{}impl<T>Raw<T>for dyn Get<T>{}fn need<T:Raw<U>+?Sized,U>(_:&T){}fn f<'a>(x:&(dyn Get<&'a u32>+'static)){need::<dyn Get<&'a u32>,_>(x)}fn main(){}"###,
            true,
        ),
        (
            "object_static_type",
            r###"fn require<T:'static>(_:T){}fn f<'a>(x:Box<dyn Iterator<Item=&'a u32>+'static>){require(x)}fn main(){}"###,
            false,
        ),
        (
            "object_any",
            r###"fn f<'a>(x:Box<dyn Iterator<Item=&'a u32>+'static>){let _:Box<dyn std::any::Any>=Box::new(x);}fn main(){}"###,
            false,
        ),
        (
            "get_static_type",
            r###"trait Get<T>{fn get(&self)->Option<T>;}fn require<T:'static>(_:T){}fn f<'a>(x:Box<dyn Get<&'a u32>+'static>){require(x)}fn main(){}"###,
            false,
        ),
        (
            "object_drop_scope",
            r###"trait Get<T>{fn get(&self)->Option<T>;}struct Empty;impl<T>Get<T>for Empty{fn get(&self)->Option<T>{None}}fn make<'a>(_:&'a u32)->Box<dyn Get<&'a u32>+'static>{Box::new(Empty)}fn main(){let object;{let x=7;object=make(&x);}drop(object);}"###,
            false,
        ),
        (
            "object_use_scope",
            r###"trait Get<T>{fn get(&self)->Option<T>;}struct Empty;impl<T>Get<T>for Empty{fn get(&self)->Option<T>{None}}fn make<'a>(_:&'a u32)->Box<dyn Get<&'a u32>+'static>{Box::new(Empty)}fn main(){let object;{let x=7;object=make(&x);}assert!(object.get().is_none());}"###,
            false,
        ),
        (
            "get_no_storage_again",
            r###"trait Get<T>{fn get(&self)->Option<T>;}struct Empty;impl<T>Get<T>for Empty{fn get(&self)->Option<T>{None}}fn make<'a>()->Box<dyn Get<&'a u32>+'static>{Box::new(Empty)}fn main(){}"###,
            true,
        ),
        (
            "generic_type_static",
            r###"trait Get<T>{fn get(&self)->Option<T>;}fn require<T:'static>(_:T){}fn f<T>(x:Box<dyn Get<T>+'static>){require(x)}fn main(){}"###,
            false,
        ),
        (
            "reference_implied",
            r###"trait Get<T>{}fn f<'a,'b>(_:&'b(dyn Get<&'a u32>+'static),x:&'a u32)->&'b u32{x}fn main(){}"###,
            true,
        ),
        (
            "generic_marker_erasure",
            r###"trait Get<T>{fn get(&self)->Option<T>;}fn erase<T>(x:Box<dyn Get<T>+Send+'static>)->Box<dyn Get<T>+'static>{x}fn main(){}"###,
            true,
        ),
        (
            "alias_iterator",
            r###"type Items<'a>=Box<dyn Iterator<Item=&'a u32>+'a>;fn values<'a>(x:&'a[u32])->Items<'a>{Box::new(x.iter())}fn main(){}"###,
            true,
        ),
        (
            "alias_swapped",
            r###"type Pairs<'b,'a>=Box<dyn Iterator<Item=(&'a u32,&'b u32)>+'a>;fn values<'a,'b:'a>(x:&'a[u32],y:&'b[u32])->Pairs<'b,'a>{Box::new(x.iter().zip(y.iter()))}fn main(){}"###,
            true,
        ),
        (
            "next_output_independent",
            r###"fn next<'a,'o>(x:&mut(dyn Iterator<Item=&'a u32>+'o))->Option<&'a u32>{x.next()}fn main(){}"###,
            true,
        ),
        (
            "next_output_wrong",
            r###"fn next<'a,'b,'o>(x:&mut(dyn Iterator<Item=&'a u32>+'o))->Option<&'b u32>{x.next()}fn main(){}"###,
            false,
        ),
        (
            "next_mut_output",
            r###"fn next<'a>(x:&mut(dyn Iterator<Item=&'a mut u32>+'a))->Option<&'a mut u32>{x.next()}fn main(){}"###,
            true,
        ),
        (
            "associated_record",
            r###"struct View<'a>(&'a u32);fn values<'a>(x:Vec<View<'a>>)->Box<dyn Iterator<Item=View<'a>>+'a>{Box::new(x.into_iter())}fn main(){}"###,
            true,
        ),
        (
            "owner_shortening",
            r###"fn shorten<'a:'o,'o>(x:Box<dyn Iterator<Item=&'a u32>+'a>)->Box<dyn Iterator<Item=&'a u32>+'o>{x}fn main(){}"###,
            true,
        ),
        (
            "marker_erasure",
            r###"fn erase<'a>(x:Box<dyn Iterator<Item=&'a u32>+Send+'a>)->Box<dyn Iterator<Item=&'a u32>+'a>{x}fn main(){}"###,
            true,
        ),
        (
            "nested_mut_invariant",
            r###"fn change<'a:'b,'b>(x:Box<dyn Iterator<Item=&'a mut &'a u32>+'a>)->Box<dyn Iterator<Item=&'b mut &'b u32>+'b>{x}fn main(){}"###,
            false,
        ),
        (
            "argument_invariant",
            r###"trait Get<T>{fn get(&self)->T;}fn change<'a:'b,'b>(x:Box<dyn Get<&'a u32>+'a>)->Box<dyn Get<&'b u32>+'b>{x}fn main(){}"###,
            false,
        ),
        (
            "no_object_implies_item_static",
            r###"fn bad<'a>(_:Box<dyn Iterator<Item=&'a u32>+'static>,x:&'a u32)->&'static u32{x}fn main(){}"###,
            false,
        ),
        (
            "generic_argument_no_storage",
            r###"trait Get<T>{fn get(&self)->Option<T>;}struct Empty;impl<T>Get<T>for Empty{fn get(&self)->Option<T>{None}}fn make<'a>()->Box<dyn Get<&'a u32>+'static>{Box::new(Empty)}fn main(){}"###,
            true,
        ),
        (
            "associated_order",
            r###"trait Pair{type A;type B;fn a(&self)->Self::A;fn b(&self)->Self::B;}struct Values<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Pair for Values<'a,'b>{type A=&'a u32;type B=&'b u32;fn a(&self)->Self::A{self.0}fn b(&self)->Self::B{self.1}}fn pair<'a:'o,'b:'o,'o>(a:&'a u32,b:&'b u32)->Box<dyn Pair<B=&'b u32,A=&'a u32>+'o>{Box::new(Values(a,b))}fn take<'a,'b>(p:&dyn Pair<B=&'b u32,A=&'a u32>)->(&'a u32,&'b u32){(p.a(),p.b())}fn main(){}"###,
            true,
        ),
        (
            "associated_order_wrong",
            r###"trait Pair{type A;type B;fn a(&self)->Self::A;fn b(&self)->Self::B;}fn bad<'a,'b>(p:&dyn Pair<B=&'b u32,A=&'a u32>)->(&'a u32,&'b u32){(p.b(),p.a())}fn main(){}"###,
            false,
        ),
        (
            "local_escape",
            r###"fn bad()->Box<dyn Iterator<Item=&'static u32>>{let x=vec![7];Box::new(x.iter())}fn main(){}"###,
            false,
        ),
        (
            "iterator_shared",
            r###"fn values<'a>(v:&'a[u32])->Box<dyn Iterator<Item=&'a u32>+'a>{Box::new(v.iter())}fn main(){}"###,
            true,
        ),
        (
            "iterator_mut",
            r###"fn values<'a>(v:&'a mut[u32])->Box<dyn Iterator<Item=&'a mut u32>+'a>{Box::new(v.iter_mut())}fn main(){}"###,
            true,
        ),
        (
            "iterator_pair",
            r###"fn values<'a>(a:&'a[String],b:&'a[u32])->Box<dyn Iterator<Item=(&'a str,&'a u32)>+'a>{Box::new(a.iter().zip(b.iter()).map(|(a,b)|(a.as_str(),b)))}fn main(){}"###,
            true,
        ),
        (
            "pair_independent",
            r###"fn values<'a:'o,'b:'o,'o>(a:&'a[u32],b:&'b[u32])->Box<dyn Iterator<Item=(&'a u32,&'b u32)>+'o>{Box::new(a.iter().zip(b.iter()))}fn main(){}"###,
            true,
        ),
        (
            "owner_differs_item",
            r###"fn values<'a,'o>(a:&'a u32)->Box<dyn Iterator<Item=&'a u32>+'o> where 'a:'o {Box::new(std::iter::once(a))}fn main(){}"###,
            true,
        ),
        (
            "item_escape",
            r###"fn values<'a>(a:&'a[u32])->Box<dyn Iterator<Item=&'static u32>+'a>{Box::new(a.iter())}fn main(){}"###,
            false,
        ),
        (
            "object_escape",
            r###"fn values<'a>(a:&'a[u32])->Box<dyn Iterator<Item=&'a u32>>{Box::new(a.iter())}fn main(){}"###,
            false,
        ),
        (
            "consume_item",
            r###"fn use_it<'a>(mut it:Box<dyn Iterator<Item=&'a u32>+'a>)->Option<&'a u32>{it.next()}fn main(){}"###,
            true,
        ),
        (
            "item_invariant",
            r###"fn change<'long:'short,'short>(it:Box<dyn Iterator<Item=&'long u32>+'long>)->Box<dyn Iterator<Item=&'short u32>+'short>{it}fn main(){}"###,
            false,
        ),
        (
            "general_arg",
            r###"trait Get<T>{fn get(&self)->T;}fn use_it<'a>(x:&dyn Get<&'a u32>)->&'a u32{x.get()}fn main(){}"###,
            true,
        ),
        (
            "owned_baseline",
            r###"fn values(v:Vec<u32>)->Box<dyn Iterator<Item=u32>>{Box::new(v.into_iter())}fn main(){}"###,
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let errors = errors(name, source);
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn signature_object_rows_are_complete_and_ordered() {
    let source = "trait Pair{type A;type B;fn a(&self)->Self::A;fn b(&self)->Self::B;}trait Get<'a>{fn get(&self)->Box<dyn Pair<B=&'static u32,A=&'a u32>+'a>;}fn main(){}";
    let mut db = SourceDb::new();
    let file = db.add_with_edition("/object-arguments/rows.rs", source, Edition::E2021);
    let hir = flux_hir::lower_sources(
        &mut db,
        &[SourceCrate {
            root: file,
            name: Symbol::intern("rows"),
            dependencies: vec![],
        }],
    );
    let typed = flux_ty::check_file(&hir);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let (owner, data) = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "Get")
        .unwrap();
    let method = data.functions[0];
    assert!(
        typed.diagnostics.is_empty(),
        "{:?}; profile {:?}",
        typed.diagnostics,
        flux_ty::regions::required_method_trait_profile(&hir, &typed.sigs, owner)
    );
    assert!(flux_ty::regions::required_method_trait_profile(&hir, &typed.sigs, owner).is_ok());
    let original = &typed.sigs.regions.fns[method].ret;
    assert_eq!(original.len(), 3);
    assert_eq!(original[0], original[1]);
    assert_ne!(original[1], original[2]);
    for corruption in 0..3 {
        let mut sigs = typed.sigs.clone();
        let ret = &mut sigs.regions.fns.get_mut(method).unwrap().ret;
        match corruption {
            0 => *ret = ret[..2].into(),
            1 => ret.swap(1, 2),
            _ => ret[2] = flux_ty::regions::Region::Erased,
        }
        assert!(
            flux_ty::regions::required_method_trait_profile(&hir, &sigs, owner).is_err(),
            "corruption {corruption}"
        );
    }
}
