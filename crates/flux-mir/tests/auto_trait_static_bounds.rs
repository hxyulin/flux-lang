//! Conditional auto-trait impls must enforce their static prerequisites everywhere.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn static_auto_trait_applicability_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-auto-static-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "compound_method_outlives",
            r#"enum Either<L,R>{Left(L),Right(R)}
impl<L,R,T> AsRef<[T]> for Either<L,R> where L:AsRef<[T]>,R:AsRef<[T]>{
 fn as_ref(&self)->&[T]{match self {Self::Left(l)=>l.as_ref(),Self::Right(r)=>r.as_ref()}}
}
impl<L,R,T> AsMut<[T]> for Either<L,R> where L:AsMut<[T]>,R:AsMut<[T]>{
 fn as_mut(&mut self)->&mut[T]{match self {Self::Left(l)=>l.as_mut(),Self::Right(r)=>r.as_mut()}}
}
fn main(){}
"#,
            true,
        ),
        (
            "type_only_trait_parameter_projection",
            r#"trait Error {}
trait Visitor<'de> { type Value; }
trait Deserializer<'de>: Sized {
 type Error: Error;
 fn deserialize<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error>;
}
trait IntoDeserializer<'de, E: Error> { type Deserializer: Deserializer<'de, Error=E>; }
struct Seq<I,E>{iter:I,error:std::marker::PhantomData<E>}
impl<'de,I,T,E> Deserializer<'de> for Seq<I,E>
where I:Iterator<Item=T>,T:IntoDeserializer<'de,E>,E:Error {
 type Error=E;
 fn deserialize<V:Visitor<'de>>(self,visitor:V)->Result<V::Value,Self::Error>{loop{}}
}
fn main(){}
"#,
            true,
        ),
        (
            "projected_marker_self_wellformedness",
            r#"use std::marker::PhantomData;use std::pin::Pin;
trait Select{type Value:?Sized;}struct Helper<T:?Sized>(T);impl<T:?Sized>Select for Helper<T>{type Value=T;}
struct Outer<'a,T>{field:Pin<&'a mut T>}
struct Origin<'p,'a,T>{dummy:PhantomData<&'p ()>,field:Pin<&'a mut T>}
impl<'p,'a,T>Unpin for Outer<'a,T>where <Helper<Origin<'p,'a,T>>as Select>::Value:Unpin{}
fn need<T:Unpin>(_:T){}fn check<'a,T>(value:Outer<'a,T>){need(value);}fn main(){}"#,
            true,
        ),
        (
            "concrete",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
fn probe(){need(Guard(5u32));}
fn main(){}
"#,
            true,
        ),
        (
            "borrowed_call",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
fn probe<'a>(x:&'a u32){need(Guard(x));}
fn main(){}
"#,
            false,
        ),
        (
            "static_call",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
fn probe<'a:'static>(x:&'a u32){need(Guard(x));}
fn main(){}
"#,
            true,
        ),
        (
            "generic_unbounded",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
fn probe<T>(x:T){need(Guard(x));}
fn main(){}
"#,
            false,
        ),
        (
            "generic_static",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
fn probe<T:'static>(x:T){need(Guard(x));}
fn main(){}
"#,
            true,
        ),
        (
            "field_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
struct Needs<T:Unpin>(T); struct Bad<'a>(Needs<Guard<&'a u32>>);
fn main(){}
"#,
            false,
        ),
        (
            "field_static",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
struct Needs<T:Unpin>(T); struct Good<'a:'static>(Needs<Guard<&'a u32>>);
fn main(){}
"#,
            true,
        ),
        (
            "supertrait_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
trait Required:Unpin{} impl<T> Required for Guard<T>{}
fn main(){}
"#,
            false,
        ),
        (
            "supertrait_good",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
trait Required:Unpin{} impl<T:'static> Required for Guard<T>{}
fn main(){}
"#,
            true,
        ),
        (
            "associated_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
trait Has{type Out:Unpin;}struct Owner<T>(T);impl<T> Has for Owner<T>{type Out=Guard<T>;}
fn main(){}
"#,
            false,
        ),
        (
            "associated_good",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{} fn need<T:Unpin>(_:T){}
trait Has{type Out:Unpin;}struct Owner<T>(T);impl<T:'static> Has for Owner<T>{type Out=Guard<T>;}
fn main(){}
"#,
            true,
        ),
        (
            "method_stricter",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Required{fn f<T>();}impl Required for (){fn f<T>()where Guard<T>:Unpin{}}
fn main(){}
"#,
            false,
        ),
        (
            "method_static",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Required{fn f<T:'static>();}impl Required for (){fn f<T:'static>()where Guard<T>:Unpin{}}
fn main(){}
"#,
            true,
        ),
        (
            "impl_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}struct Marker<T>(T);impl<'a> Needs<Guard<&'a u32>> for Marker<&'a u32>{}
fn main(){}
"#,
            false,
        ),
        (
            "trait_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}trait Bad<'a>:Needs<Guard<&'a u32>>{}
fn main(){}
"#,
            false,
        ),
        (
            "pointer_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
fn require<T:Unpin>(){}fn f<'a>(){let _:fn()=require::<Guard<&'a u32>>;}
fn main(){}
"#,
            false,
        ),
        (
            "opaque_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
fn make<'a>(x:&'a u32)->impl Unpin {Guard(x)}
fn main(){}
"#,
            false,
        ),
        (
            "projection_bad",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Get{type Out;}impl<T:Unpin> Get for T{type Out=u32;}fn f<'a>()-><Guard<&'a u32>as Get>::Out{0}
fn main(){}
"#,
            false,
        ),
        (
            "function_trait_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}fn f<'a,X:Needs<Guard<&'a u32>>>(){}
fn main(){}
"#,
            false,
        ),
        (
            "function_trait_argument_static",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}fn f<'a:'static,X:Needs<Guard<&'a u32>>>(){}
fn main(){}
"#,
            true,
        ),
        (
            "trait_parameter_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}trait Bad<'a,X:Needs<Guard<&'a u32>>>{}
fn main(){}
"#,
            false,
        ),
        (
            "alias_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
struct Needs<T:Unpin>(T);type Bad<'a>=Needs<Guard<&'a u32>>;
fn main(){}
"#,
            true,
        ),
        (
            "separate_static_parameter",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
struct Pair<T:?Sized,A>(Box<T>,A);impl<T:?Sized,A:'static>Unpin for Pair<T,A>{}fn f<'a>(x:&'a u32){need(Pair(Box::new(x),0u8));}fn need<T:Unpin>(_:T){}
fn main(){}
"#,
            true,
        ),
        (
            "separate_borrowed_parameter",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
struct Pair<T:?Sized,A>(Box<T>,A);impl<T:?Sized,A:'static>Unpin for Pair<T,A>{}fn f<'a>(x:&'a u32){need(Pair(Box::new(0u8),x));}fn need<T:Unpin>(_:T){}
fn main(){}
"#,
            false,
        ),
        (
            "unrelated_non_unpin_parameter",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
struct Pair<T:?Sized,A>(Box<T>,A);impl<T:?Sized,A:'static>Unpin for Pair<T,A>{}fn f(){need(Pair(Box::new(std::marker::PhantomPinned),0u8));}fn need<T:Unpin>(_:T){}
fn main(){}
"#,
            true,
        ),
        (
            "function_generic_argument",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}fn f<T,X:Needs<Guard<T>>>(){}
fn main(){}
"#,
            false,
        ),
        (
            "function_generic_argument_static",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Needs<T:Unpin>{}fn f<T:'static,X:Needs<Guard<T>>>(){}
fn main(){}
"#,
            true,
        ),
        (
            "method_type_stricter",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Methods{fn f<T>();}impl Methods for (){fn f<T:'static>(){}}
fn main(){}
"#,
            false,
        ),
        (
            "method_type_implied",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Methods{fn f<'a,T>(_:&'a T);}impl Methods for (){fn f<'a,T:'a>(_:&'a T){}}
fn main(){}
"#,
            false,
        ),
        (
            "method_type_named",
            r#"struct Guard<T>(T); impl<T: 'static> Unpin for Guard<T>{}
trait Methods{fn f<'a,T:'a>(_:&'a ());}impl Methods for (){fn f<'a,T:'a>(_:&'a ()){}}
fn main(){}
"#,
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
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
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
