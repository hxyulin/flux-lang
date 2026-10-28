//! Concrete associated field types retain the lifetimes of their written inputs.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn qualified_lifetimes_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-qualified-lifetimes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, source) in [
        (
            "qualified_receiver",
            true,
            "trait Take{fn take(self)->Self;}impl<'a> Take for &'a u32{fn take(self)->Self{self}}fn get<'a>(x:&'a u32)->&'a u32{<&'a u32>::take(x)}fn main(){}",
        ),
        (
            "receiver_escape",
            false,
            "trait Take{fn take(self)->Self;}impl<'a> Take for &'a u32{fn take(self)->Self{self}}fn get<'a>(x:&'a u32)->&'static u32{<&'a u32>::take(x)}fn main(){}",
        ),
        (
            "receiver_static_input",
            false,
            "trait Take{fn take(self)->Self;}impl<'a> Take for &'a u32{fn take(self)->Self{self}}fn get<'a>(x:&'a u32)->&'a u32{<&'static u32>::take(x)}fn main(){}",
        ),
        (
            "receiver_value",
            true,
            "fn get<'a>(x:&'a [u32;2])->[u32;2]{<&'a [u32;2]>::try_from(&x[..]).unwrap().clone()}fn main(){}",
        ),
        (
            "nested_abstract",
            true,
            "trait First{type Next:Second;}trait Second{type Out;}struct S<'a,T:First>{iter:core::slice::Iter<'a,<<T as First>::Next as Second>::Out>}fn main(){}",
        ),
        (
            "nested_field",
            true,
            "struct S<'a>{v:Option<<&'a [u32] as IntoIterator>::IntoIter>}fn get<'a>(s:S<'a>)->&'a u32{s.v.unwrap().as_slice().first().unwrap()}fn main(){}",
        ),
        (
            "unused_selected_input",
            true,
            "trait Tr{type Out;}impl<T>Tr for Vec<T>{type Out=u32;}struct S<'a>{v:<Vec<&'a u32> as Tr>::Out}fn main(){}",
        ),
        (
            "swapped_regions",
            true,
            "trait Swap{type Out;}impl<'a,'b>Swap for (&'a u32,&'b u32){type Out=(&'b u32,&'a u32);}struct S<'a,'b>{v:<(&'a u32,&'b u32)as Swap>::Out}fn get<'a,'b>(s:S<'a,'b>)->(&'b u32,&'a u32){s.v}fn main(){}",
        ),
        (
            "swapped_escape",
            false,
            "trait Swap{type Out;}impl<'a,'b>Swap for (&'a u32,&'b u32){type Out=(&'b u32,&'a u32);}struct S<'a,'b>{v:<(&'a u32,&'b u32)as Swap>::Out}fn get<'a,'b>(s:S<'a,'b>)->(&'a u32,&'b u32){s.v}fn main(){}",
        ),
        (
            "projection_invariance",
            false,
            "trait Tr{type Out;}impl<T>Tr for Vec<T>{type Out=u32;}struct S<'a>{v:<Vec<&'a u32> as Tr>::Out}fn shorten<'a:'b,'b>(s:S<'a>)->S<'b>{s}fn main(){}",
        ),
        (
            "selected_bound_required",
            false,
            "trait Tr{type Out;}impl<'a:'b,'b>Tr for (&'a u32,&'b u32){type Out=u32;}struct S<'a,'b>{v:<(&'a u32,&'b u32)as Tr>::Out}fn main(){}",
        ),
        (
            "selected_implied_bound",
            true,
            "trait Tr{type Out;}impl<'a,'b>Tr for &'a &'b u32{type Out=u32;}struct S<'a,'b>{v:<&'a &'b u32 as Tr>::Out}fn prove<'a,'b>(_:S<'a,'b>,x:&'b u32)->&'a u32{x}fn main(){}",
        ),
        (
            "selected_implied_caller",
            false,
            "trait Tr{type Out;}impl<'a,'b>Tr for &'a &'b u32{type Out=u32;}struct S<'a,'b>{v:<&'a &'b u32 as Tr>::Out}fn make<'a,'b>(_:&'a u32,_:&'b u32)->S<'a,'b>{S{v:0}}fn prove<'a,'b>(_:S<'a,'b>,x:&'b u32)->&'a u32{x}static A:u32=1;fn main(){let b=2;let s=make(&A,&b);let r:&'static u32=prove(s,&b);assert_eq!(*r,2);}",
        ),
        (
            "qualified_self",
            true,
            "trait Take{fn take(self)->Self;}impl<'a,T>Take for &'a T{fn take(self)->Self{self}}struct S(u32);impl S{fn get<'a>(&'a self)->&'a Self{<&'a Self>::take(self)}}fn main(){}",
        ),
        (
            "qualified_mutable",
            true,
            "trait Take{fn take(self)->Self;}impl<'a,T>Take for &'a mut T{fn take(self)->Self{self}}fn get<'a,T>(x:&'a mut T)->&'a mut T{<&'a mut T>::take(x)}fn main(){}",
        ),
        (
            "outer_reference_requires_bound",
            false,
            "struct S<'a,'b>{v:&'a <&'b [u32] as IntoIterator>::IntoIter}fn main(){}",
        ),
        (
            "nested_selection_input_invariance",
            false,
            "trait Sink{type Out;}impl<T>Sink for T{type Out=u32;}struct S<'a>{v:<<&'a [u32]as IntoIterator>::IntoIter as Sink>::Out,r:&'a u32}fn shorten<'a:'b,'b>(s:S<'a>)->S<'b>{s}fn main(){}",
        ),
        (
            "owned_iter",
            true,
            "struct Content<'a>(&'a u32); struct S<'a>{iter:<Vec<Content<'a>> as IntoIterator>::IntoIter} fn main(){}",
        ),
        (
            "slice_iter",
            true,
            "struct Content<'a>(&'a u32); struct S<'a,'b>{iter:<&'a [Content<'b>] as IntoIterator>::IntoIter} fn main(){}",
        ),
        (
            "field_return",
            true,
            "struct S<'a>{iter:<&'a [u32] as IntoIterator>::IntoIter} fn get<'a>(s:S<'a>)->&'a u32{s.iter.as_slice().first().unwrap()} fn main(){}",
        ),
        (
            "field_escape",
            false,
            "struct S<'a>{iter:<&'a [u32] as IntoIterator>::IntoIter} fn get<'a>(s:S<'a>)->&'static u32{s.iter.as_slice().first().unwrap()} fn main(){}",
        ),
        (
            "field_invariant",
            false,
            "struct S<'a>{iter:<&'a mut [u32] as IntoIterator>::IntoIter} fn get<'a>(s:&mut S<'a>,x:&'a mut [u32]){s.iter=x.into_iter();} fn main(){let mut a=[1];let mut s=S{iter:a.iter_mut()};{let mut b=[2];get(&mut s,&mut b);}let _=s.iter.next();}",
        ),
        (
            "unused_lifetime",
            false,
            "trait Tr{type Out;}impl Tr for u32{type Out=u32;}struct S<'a>{v:<u32 as Tr>::Out}fn main(){}",
        ),
        (
            "selected_static",
            true,
            "trait Tr{type Out;}impl<T> Tr for Vec<T>{type Out=&'static u32;}struct S{v:<Vec<u32> as Tr>::Out}fn get(s:S)->&'static u32{s.v}fn main(){}",
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-Awarnings"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let diagnostics = if typed.diagnostics.is_empty() {
            flux_mir::build_file(&hir, &typed).diagnostics
        } else {
            typed.diagnostics
        };
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
