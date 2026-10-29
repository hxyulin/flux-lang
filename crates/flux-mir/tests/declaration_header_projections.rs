//! Selected header projections preserve their own syntax input regions.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn projected_headers_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-projected-headers-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "slice_offset_projection",
            "trait Offset<S=Self>{}trait Stream:Offset<Self::Checkpoint>{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{}impl<'a,T>Stream for &'a[T]{type Checkpoint=Checkpoint<Self,Self>;}",
            true,
        ),
        (
            "repeated_header_projection",
            "trait A{type X;}struct S<'a,'b>(&'a u32,&'b u32);impl<'a,'b>A for S<'a,'b>{type X=(&'a u32,&'b u32);}trait P<X,Y>{}trait D<X,Y>:P<X,Y>{}impl<'a,'b>P<<S<'a,'b>as A>::X,<S<'a,'b>as A>::X>for S<'a,'b>{}impl<'a,'b>D<(&'a u32,&'b u32),(&'a u32,&'b u32)>for S<'a,'b>{}",
            true,
        ),
        (
            "repeated_header_projection_distinct",
            "trait A{type X;}struct S<'a,'b>(&'a u32,&'b u32);impl<'a,'b>A for S<'a,'b>{type X=(&'a u32,&'b u32);}trait P<X,Y>{}trait D<X,Y>:P<X,Y>{}impl<'a,'b>P<<S<'a,'b>as A>::X,<S<'b,'a>as A>::X>for S<'a,'b>{}impl<'a,'b>D<(&'a u32,&'b u32),(&'a u32,&'b u32)>for S<'a,'b>{}",
            false,
        ),
        (
            "selected_static_prerequisite_missing",
            "trait A{type X;}struct S<'a>(&'a u32);impl<'a:'static>A for S<'a>{type X=u8;}trait P<X>{}trait D<X>:P<X>+A{}impl<'a:'static>P<<S<'a>as A>::X>for S<'a>{}impl<'a>D<u8>for S<'a>{}",
            false,
        ),
        (
            "selected_static_prerequisite_present",
            "trait A{type X;}struct S<'a>(&'a u32);impl<'a:'static>A for S<'a>{type X=u8;}trait P<X>{}trait D<X>:P<X>+A{}impl<'a:'static>P<<S<'a>as A>::X>for S<'a>{}impl<'a:'static>D<u8>for S<'a>{}",
            true,
        ),
        (
            "nested_iterator_rhs",
            "trait Stream{type Token;type Iter:Iterator<Item=(usize,Self::Token)>;}impl<'a,T:Clone>Stream for &'a[T]{type Token=T;type Iter=core::iter::Enumerate<core::iter::Cloned<core::slice::Iter<'a,T>>>;}",
            true,
        ),
        (
            "nested_iterator_rhs_wrong_item",
            "trait Stream{type Token;type Iter:Iterator<Item=(usize,Self::Token)>;}impl<'a,T:Clone>Stream for &'a[T]{type Token=u8;type Iter=core::iter::Enumerate<core::iter::Cloned<core::slice::Iter<'a,T>>>;}",
            false,
        ),
        (
            "nested_iterator_rhs_missing_bound",
            "trait Stream{type Token;type Iter:Iterator<Item=(usize,Self::Token)>;}impl<'a,T>Stream for &'a[T]{type Token=T;type Iter=core::iter::Enumerate<core::iter::Cloned<core::slice::Iter<'a,T>>>;}",
            false,
        ),
        (
            "independent_named_lifetime",
            "trait A{type X;}struct S<'a>(&'a u32);impl<'a>A for S<'a>{type X=&'a u32;}trait P<X>{}trait D<X>:P<X>{}impl<'a,'b>P<<S<'a>as A>::X>for S<'b>{}impl<'b>D<&'b u32>for S<'b>{}",
            true,
        ),
        (
            "independent_static_missing",
            "trait A{type X;}struct S<'a>(&'a u32);impl<'a>A for S<'a>{type X=&'a u32;}trait P<X>{}trait D<X>:P<X>{}impl<'a:'static,'b>P<<S<'a>as A>::X>for S<'b>{}impl<'b>D<&'b u32>for S<'b>{}",
            false,
        ),
        (
            "independent_static_present",
            "trait A{type X;}struct S<'a>(&'a u32);impl<'a>A for S<'a>{type X=&'a u32;}trait P<X>{}trait D<X>:P<X>{}impl<'a:'static,'b>P<<S<'a>as A>::X>for S<'b>{}impl<'b:'static>D<&'b u32>for S<'b>{}",
            true,
        ),
        (
            "independent_anonymous_lifetime",
            "trait Offset<S=Self>{}trait Stream:Offset<Self::Checkpoint>{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Offset<<&[T]as Stream>::Checkpoint>for &[T]{}impl<'a,T>Stream for &'a[T]{type Checkpoint=Checkpoint<Self,Self>;}",
            true,
        ),
        (
            "existential_mut_swapped_present",
            "trait A{type X;}struct S<'a,'b>(&'a mut u32,&'b mut u32);impl<'a,'b>A for S<'a,'b>{type X=(&'b mut u32,&'a mut u32);}trait P<X>{}trait D<X>:P<X>{}impl<'a:'static,'b,'c,'d>P<<S<'a,'b>as A>::X>for S<'c,'d>{}impl<'c,'d:'static>D<(&'c mut u32,&'d mut u32)>for S<'c,'d>{}",
            true,
        ),
        (
            "existential_mut_swapped_wrong_static",
            "trait A{type X;}struct S<'a,'b>(&'a mut u32,&'b mut u32);impl<'a,'b>A for S<'a,'b>{type X=(&'b mut u32,&'a mut u32);}trait P<X>{}trait D<X>:P<X>{}impl<'a:'static,'b,'c,'d>P<<S<'a,'b>as A>::X>for S<'c,'d>{}impl<'c:'static,'d>D<(&'c mut u32,&'d mut u32)>for S<'c,'d>{}",
            false,
        ),
        (
            "inherited_concrete_outputs",
            "trait Offset<S=Self>{}trait Stream:Offset<Self::Checkpoint>{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{}impl<'a,T>Stream for &'a[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Update:Stream{}impl<'a,T>Update for &'a[T]{}",
            true,
        ),
        (
            "boxed_iterator_output",
            "struct S(Vec<u32>);impl IntoIterator for S{type Item=u32;type IntoIter=Box<dyn Iterator<Item=u32>>;fn into_iter(self)->Self::IntoIter{loop{}}}",
            true,
        ),
        (
            "borrowed_boxed_iterator_output",
            "struct S(Vec<u32>);impl<'a>IntoIterator for &'a S{type Item=&'a u32;type IntoIter=Box<dyn Iterator<Item=&'a u32>+'a>;fn into_iter(self)->Self::IntoIter{loop{}}}",
            true,
        ),
        (
            "boxed_iterator_wrong_item",
            "struct S(Vec<u32>);impl IntoIterator for S{type Item=u8;type IntoIter=Box<dyn Iterator<Item=u32>>;fn into_iter(self)->Self::IntoIter{loop{}}}",
            false,
        ),
        (
            "borrowed_boxed_iterator_wrong_lifetime",
            "struct S(Vec<u32>);impl<'a>IntoIterator for &'a S{type Item=&'static u32;type IntoIter=Box<dyn Iterator<Item=&'a u32>+'a>;fn into_iter(self)->Self::IntoIter{loop{}}}",
            false,
        ),
        (
            "borrowed_boxed_iterator_static",
            "struct S(Vec<u32>);impl<'a:'static>IntoIterator for &'a S{type Item=&'static u32;type IntoIter=Box<dyn Iterator<Item=&'a u32>+'a>;fn into_iter(self)->Self::IntoIter{loop{}}}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}
