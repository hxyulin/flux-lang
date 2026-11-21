//! Associated output bounds retain their owning trait occurrence lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn associated_output_bounds_match_rustc() {
    let dir = std::env::temp_dir().join(format!(
        "flux-associated-output-bounds-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, accepted, source) in [
        (
            "enclosing_blanket_impl",
            true,
            include_str!("../../../tests/run/enclosing_blanket_impl.rs"),
        ),
        (
            "enclosing_blanket_impl_cannot_change_its_owner_lifetime",
            false,
            "trait Marker<'a>{}trait Collect<'a>{fn known(value:&'a u32);fn bad<'b>(value:&'b u32);}impl<'a,T:Marker<'a>> Collect<'a> for T{fn known(_:&'a u32){}fn bad<'b>(value:&'b u32){<T as Collect<'b>>::known(value);}}fn main(){}",
        ),
        (
            "inherited_iterator_projection",
            true,
            include_str!("../../../tests/run/inherited_iterator_projection.rs"),
        ),
        (
            "inherited_iterator_projection_cannot_change_lifetimes",
            false,
            "trait Components<'a>:Iterator<Item=Self::Component>{type Component;}impl<'a> Components<'a> for std::iter::Once<&'a str>{type Component=&'static str;}fn main(){}",
        ),
        (
            "logos_source_lifetimes",
            true,
            include_str!("../../../tests/run/logos_source_lifetimes.rs"),
        ),
        (
            "opaque_iterator_projection",
            true,
            include_str!("../../../tests/run/opaque_iterator_projection.rs"),
        ),
        (
            "opaque_projection_cannot_escape",
            false,
            "trait Source{type Output;fn output(&self)->Self::Output;}struct Ref<'a>(&'a u32);impl<'a> Source for Ref<'a>{type Output=&'a u32;fn output(&self)->Self::Output{self.0}}fn opaque(value:&u32)->impl Source+'_ {Ref(value)}fn require_static<T:'static>(_:T){}fn check(value:&u32){require_static(opaque(value).output());}fn main(){}",
        ),
        (
            "trait_owner_wrong_lifetime",
            false,
            "trait Token<'a>{}enum E<'a,T:Token<'a>>{Value(T),Borrowed(&'a())}trait Callback<'a,'b,T:Token<'a>>{fn get()->E<'b,T>;}fn main(){}",
        ),
        (
            "family_default_self_wrong_lifetime",
            false,
            "trait Same<Rhs=Self>{}impl<T>Same for T{}trait G:'static{type Out<'a>:Same<Self::Out<'static>>where Self:'a;}struct S;impl G for S{type Out<'a>=&'a str;}fn main(){}",
        ),
        (
            "associated_source_escape",
            false,
            "trait Source{type Slice<'a>where Self:'a;fn slice(&self)->Self::Slice<'_>;}trait Token<'s>{type Source:Source+?Sized+'s;}struct Lexer<'s,T:Token<'s>>{source:&'s T::Source}impl<'s,T:Token<'s>> Lexer<'s,T>{fn escape(&self)-><T::Source as Source>::Slice<'static>{self.source.slice()}}fn main(){}",
        ),
        (
            "generic_vector_index_cannot_escape",
            false,
            "use std::ops::Index;use std::slice::SliceIndex;struct Entry<'a>(&'a str);fn escape<'a,I:SliceIndex<[Entry<'a>]>>(values:Vec<Entry<'a>>,index:I)->&'a I::Output{values.index(index)}fn main(){}",
        ),
        (
            "forward_output_object_argument",
            true,
            include_str!("../../../tests/run/associated_bound_forwarding.rs"),
        ),
        (
            "projected_object_argument_outlives",
            true,
            include_str!("../../../tests/run/trait_object_associated_lifetime.rs"),
        ),
        (
            "projected_object_argument_requires_outlives",
            false,
            "type Items<'a,T>=Box<dyn Iterator<Item=T>+'a>;trait Source<'a>{type Item;fn items(&self)->Items<'a,Self::Item>;}fn collect<'a,S:Source<'a>>(source:&S)->Items<'a,Option<S::Item>>{Box::new(source.items().map(|item|Some(item)))}fn main(){}",
        ),
        (
            "repeated_projected_bound_cannot_extend_owner",
            false,
            "trait Source<'a>{type Item:'a;}fn escape<'a,S:Source<'a>>(value:&'a u8)->&'static u8 where S::Item:'a{value}fn main(){}",
        ),
        (
            "normalized_projection_argument",
            true,
            include_str!("../../../tests/run/normalized_projection_argument.rs"),
        ),
        (
            "projected_callback_lifetimes",
            true,
            include_str!("../../../tests/run/projected_callback_lifetimes.rs"),
        ),
        (
            "projected_callback_cannot_return_mutable_capture",
            false,
            "type Items<'a,T>=Box<dyn Iterator<Item=T>+'a>;trait Source<'a>{type Item:'a;}fn invalid<'a,S:Source<'a>,F:FnMut(&S::Item)->Items<'a,S::Item>+'a>(items:Items<'a,S::Item>,mut callback:F,mut owned:Vec<i32>)->Items<'a,&'a mut i32>{Box::new(items.map(move|item|{let _=callback(&item);&mut owned[0]}))}fn main(){}",
        ),
        (
            "generic_flatten_projection",
            true,
            include_str!("../../../tests/run/generic_flatten_projection.rs"),
        ),
        (
            "generic_flatten_cannot_outlive_its_input",
            false,
            "fn flatten<T>(_:T, values:&[Option<Vec<String>>])->Box<dyn Iterator<Item=usize>+'static>{let rows=values.iter().filter_map(|value|match value{Some(value)=>Some(value.as_slice()),None=>None});Box::new(rows.flatten().map(|value|value.len()))}fn main(){}",
        ),
        (
            "projected_iterator_outlives",
            true,
            include_str!("../../../tests/run/projected_iterator_outlives.rs"),
        ),
        (
            "projected_iterator_requires_declared_outlives",
            false,
            "type Items<'a,T>=Box<dyn Iterator<Item=T>+'a>;trait Source<'a>{type Item;}struct Expand<'a,T:'a>{inner:Items<'a,T>}impl<'a,T:'a>Iterator for Expand<'a,T>{type Item=T;fn next(&mut self)->Option<T>{self.inner.next()}}fn flatten<'a,S:Source<'a>>(items:Items<'a,(S::Item,Items<'a,S::Item>)>)->Items<'a,S::Item>{Box::new(items.flat_map(|(_,inner)|Expand{inner}))}fn main(){}",
        ),
        (
            "projection_argument_requires_well_formed_owner",
            false,
            "trait Mark<'a>{}impl Mark<'static> for (){}struct Guard<'a,T:Mark<'a>>(T,core::marker::PhantomData<&'a()>);trait Read{type Out;}impl<'a,T:Mark<'a>>Read for Guard<'a,T>{type Out=u8;}fn check<T>(){}fn probe<'a>(_:&'a()){check::<<Guard<'a,()> as Read>::Out>();}fn main(){}",
        ),
        (
            "forward_output_more_general_argument",
            false,
            "trait Source{type Out:Into<&'static u8>;}trait Target{type Out:for<'a> Into<&'a u8>;}impl<T:Source>Target for Box<T>{type Out=T::Out;}fn main(){}",
        ),
        (
            "forward_output",
            true,
            "trait Error{}trait Access<'a>{type Error:Error;}impl<'a,A:Access<'a>+?Sized>Access<'a>for &mut A{type Error=A::Error;}fn main(){}",
        ),
        (
            "forward_output_supertrait",
            true,
            "trait Error{}trait Detail:Error{}trait Access<'a>{type Error:Detail;}trait Target{type Error:Error;}struct W<A>(A);impl<'a,A:Access<'a>>Target for W<&'a A>{type Error=A::Error;}fn main(){}",
        ),
        (
            "forward_output_trait_lifetime",
            true,
            "trait Mark<'a>{}trait Access<'a>{type Out:Mark<'a>;}struct W<A>(A);impl<'a,A:Access<'a>>Access<'a>for W<A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_wrong_trait_lifetime",
            false,
            "trait Mark<'a>{}trait Access<'a>{type Out:Mark<'a>;}struct W<A>(A);impl<'a,'b,A:Access<'a>>Access<'b>for W<A>{type Out=<A as Access<'a>>::Out;}fn main(){}",
        ),
        (
            "forward_output_static_required",
            false,
            "trait Mark<'a>{}trait Access<'a>{type Out:Mark<'a>;}trait Target{type Out:Mark<'static>;}struct W<'a,A>(&'a A);impl<'a,A:Access<'a>>Target for W<'a,A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_wrong_bound",
            false,
            "trait Error{}trait Other{}trait Access<'a>{type Out:Error;}trait Target{type Out:Other;}struct W<'a,A>(&'a A);impl<'a,A:Access<'a>>Target for W<'a,A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_ambiguous_owners",
            false,
            "trait Mark<'a>{}trait Access<'a>{type Out:Mark<'a>;}struct W<'a,A>(&'a A);impl<'a,'b,A:Access<'a>+Access<'b>>Access<'a>for W<'b,A>{type Out=<A as Access<'a>>::Out;}fn main(){}",
        ),
        (
            "forward_output_wrong_owner",
            false,
            "trait Mark<'a>{}trait Access<'a>{type Out:Mark<'a>;}struct W<'a,A>(&'a A);impl<'a,'b,A:Access<'a>+Access<'b>>Access<'a>for W<'b,A>{type Out=<A as Access<'b>>::Out;}fn main(){}",
        ),
        (
            "forward_output_type_argument",
            true,
            "trait Mark<T>{}trait Access<'a,T>{type Out:Mark<T>;}struct W<A>(A);impl<'a,T,A:Access<'a,T>>Access<'a,T>for W<A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_reference_argument",
            true,
            "trait Mark<T>{}trait Access<'a,T>{type Out:Mark<T>;}struct W<A>(A);impl<'a,T:'a,A:Access<'a,&'a T>>Access<'a,&'a T>for W<A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_swapped_arguments",
            true,
            "trait Mark<X,Y>{}trait Access<'a,X,Y>{type Out:Mark<Y,X>;}struct W<A>(A);impl<'a,X,Y,A:Access<'a,X,Y>>Access<'a,X,Y>for W<A>{type Out=A::Out;}fn main(){}",
        ),
        (
            "forward_output_wrong_argument",
            false,
            "trait Mark<T>{}trait Access<'a,T>{type Out:Mark<T>;}trait Target<T>{type Out:Mark<T>;}struct W<'a,A,X>(&'a A,core::marker::PhantomData<X>);impl<'a,X,Y,A:Access<'a,X>>Target<Y>for W<'a,A,X>{type Out=A::Out;}fn main(){}",
        ),
        (
            "associated_trait_argument_method",
            true,
            "trait Source<'a>{type Error;}struct W<A>(A);trait Convert<'a,E>{type Output;fn convert(self)->Self::Output;}impl<'a,A:Source<'a>>Convert<'a,A::Error>for W<A>{type Output=Self;fn convert(self)->Self{self}}fn main(){}",
        ),
        (
            "associated_argument_return_link",
            true,
            "trait Source<'a>{type Error;}struct W<'a,A:Source<'a>>(&'a A::Error);trait Convert<'a,E>{fn get<'b>(&'b self)->&'a E;}impl<'a,A:Source<'a>>Convert<'a,A::Error>for W<'a,A>{fn get<'b>(&'b self)->&'a A::Error{self.0}}fn main(){}",
        ),
        (
            "associated_argument_wrong_return_link",
            false,
            "trait Source<'a>{type Error;}struct W<'a,A:Source<'a>>(&'a A::Error);trait Convert<'a,E>{fn get<'b>(&'b self)->&'a E;}impl<'a,A:Source<'a>>Convert<'a,A::Error>for W<'a,A>{fn get<'b>(&'b self)->&'b A::Error{self.0}}fn main(){}",
        ),
        (
            "family_iterator_output",
            true,
            "trait Size{type Array<T>:IntoIterator<Item=T>;}struct W<T,U:Size>(U::Array<T>);impl<T,U:Size>IntoIterator for W<T,U>{type Item=T;type IntoIter=<U::Array<T>as IntoIterator>::IntoIter;fn into_iter(self)->Self::IntoIter{self.0.into_iter()}}fn main(){}",
        ),
        (
            "family_lending_output",
            true,
            "trait Size{type Iter<'a>:Iterator<Item=&'a u32>where Self:'a;}struct W<U>(U);trait Forward<'a>{type Iter:Iterator<Item=&'a u32>;}impl<'a,U:Size+'a>Forward<'a>for W<U>{type Iter=U::Iter<'a>;}fn main(){}",
        ),
        (
            "family_lending_missing_owner",
            false,
            "trait Size{type Iter<'a>:Iterator<Item=&'a u32>where Self:'a;}struct W<U>(U);trait Forward<'a>{type Iter:Iterator<Item=&'a u32>;}impl<'a,U:Size>Forward<'a>for W<U>{type Iter=U::Iter<'a>;}fn main(){}",
        ),
        (
            "family_type_prerequisite",
            true,
            "trait Mark{}trait G{type Out<T:Mark>:Copy;}trait Target{type Out:Copy;}struct W<A,T>(A,core::marker::PhantomData<T>);impl<T:Mark,A:G>Target for W<A,T>{type Out=A::Out<T>;}fn main(){}",
        ),
        (
            "family_wrong_type_prerequisite",
            false,
            "trait Mark{}trait G{type Out<T:Mark>:Copy;}trait Target{type Out:Copy;}struct W<A,T>(A,core::marker::PhantomData<T>);impl<T,A:G>Target for W<A,T>{type Out=A::Out<T>;}fn main(){}",
        ),
        (
            "supertrait_uses_defined_output",
            true,
            "trait Bound{}trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined:Bound;}impl<T:Base>Derived for T where T::Out:Bound{type Defined=T::Out;}fn main(){}",
        ),
        (
            "supertrait_wrong_defined_output",
            false,
            "trait Bound{}impl Bound for u32{}trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined:Bound;}impl<T:Base>Derived for T where T::Out:Bound{type Defined=u32;}fn main(){}",
        ),
        (
            "family_static_prerequisite",
            true,
            "trait G{type Out<T:'static>:Copy;}trait Target{type Out:Copy;}struct W<A,T>(A,core::marker::PhantomData<T>);impl<T:'static,A:G>Target for W<A,T>{type Out=A::Out<T>;}fn main(){}",
        ),
        (
            "family_missing_static_prerequisite",
            false,
            "trait G{type Out<T:'static>:Copy;}trait Target{type Out:Copy;}struct W<A,T>(A,core::marker::PhantomData<T>);impl<T,A:G>Target for W<A,T>{type Out=A::Out<T>;}fn main(){}",
        ),
        (
            "circular_output_bounds",
            false,
            "trait Bound{}trait A<'a>{type Out:Bound;}trait B<'a>{type Out:Bound;}struct S;impl<'a>A<'a>for S{type Out=<Self as B<'a>>::Out;}impl<'a>B<'a>for S{type Out=<Self as A<'a>>::Out;}fn main(){}",
        ),
        (
            "associated_argument_independent_lifetimes",
            true,
            "trait Source<'s>{type Error;}struct W<'s,A:Source<'s>>(&'s A::Error);trait Convert<'a,E>{fn get(&self)->&'a E;}impl<'a,'s:'a,A:Source<'s>>Convert<'a,A::Error>for W<'s,A>{fn get(&self)->&'s A::Error{self.0}}fn main(){}",
        ),
        (
            "associated_argument_missing_lifetime_link",
            false,
            "trait Source<'s>{type Error;}struct W<'s,A:Source<'s>>(&'s A::Error);trait Convert<'a,E>{fn get(&self)->&'a E;}impl<'a,'s,A:Source<'s>>Convert<'a,A::Error>for W<'s,A>{fn get(&self)->&'s A::Error{self.0}}fn main(){}",
        ),
        (
            "iterator_selected_output",
            true,
            "trait Error{}trait Deser<'a>{type Error:Error;}trait Into<'a,E:Error>{type Out:Deser<'a,Error=E>;}struct Seq<I,E>(I,core::marker::PhantomData<E>);impl<'a,I,T,E>Deser<'a>for Seq<I,E>where I:Iterator<Item=T>,T:Into<'a,E>,E:Error{type Error=E;}impl<'a,T,E>Into<'a,E>for Vec<T>where T:Into<'a,E>,E:Error{type Out=Seq<<Self as IntoIterator>::IntoIter,E>;}fn main(){}",
        ),
        (
            "iterator_selected_static_only",
            false,
            "trait Error{}trait Deser<'a>{type Error:Error;}trait Into<'a,E:Error>{type Out:Deser<'a,Error=E>;}struct Seq<I,E>(I,core::marker::PhantomData<E>);impl<'a,I,T,E>Deser<'a>for Seq<I,E>where I:Iterator<Item=T>,T:Into<'static,E>,E:Error{type Error=E;}impl<'a,T,E>Into<'a,E>for Vec<T>where T:Into<'a,E>,E:Error{type Out=Seq<<Self as IntoIterator>::IntoIter,E>;}fn main(){}",
        ),
        (
            "concrete_supertrait_definition",
            true,
            "trait Bound{}impl Bound for u32{}trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined:Bound;}impl<T:Base>Derived for T where T::Out:Bound{type Defined=T::Out;}struct S;impl Base for S{type Out=u32;}trait Outer:Derived{}impl Outer for S{}fn main(){}",
        ),
        (
            "selected_supertrait_missing_static",
            false,
            "trait Bound{}impl Bound for u32{}trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined:Bound;}impl<T:Base+'static>Derived for T where T::Out:Bound{type Defined=T::Out;}struct S<'a>(&'a u32);impl Base for S<'_>{type Out=u32;}trait Outer:Derived{}impl<'a>Outer for S<'a>{}fn main(){}",
        ),
        (
            "repeated_projection_owner_constraints",
            true,
            "trait Read<'a>{type Out;}struct S;impl Read<'static>for S{type Out=u32;}trait Mark<'a>{}impl<'a>Mark<'a>for(u32,u32){}trait Pair<'a>{type Out:Mark<'a>;}struct W<'a>(&'a());impl<'a:'static>Pair<'a>for W<'a>{type Out=(<S as Read<'static>>::Out,<S as Read<'a>>::Out);}fn main(){}",
        ),
        (
            "repeated_projection_wrong_owner",
            false,
            "trait Read<'a>{type Out;}struct S;impl Read<'static>for S{type Out=u32;}trait Mark<'a>{}impl<'a>Mark<'a>for(u32,u32){}trait Pair<'a>{type Out:Mark<'a>;}struct W<'a>(&'a());impl<'a>Pair<'a>for W<'a>{type Out=(<S as Read<'static>>::Out,<S as Read<'a>>::Out);}fn main(){}",
        ),
        (
            "defined_error_in_bounded_adt",
            true,
            "trait Error{}trait Variant<'a>{type Error:Error;}trait Enum<'a>{type Error:Error;type Variant:Variant<'a,Error=Self::Error>;}struct V<'a,E:Error>(&'a(),core::marker::PhantomData<E>);impl<'a,E:Error>Variant<'a>for V<'a,E>{type Error=E;}struct S<'a,E:Error>(&'a(),core::marker::PhantomData<E>);impl<'a,E:Error>Enum<'a>for S<'a,E>{type Error=E;type Variant=V<'a,Self::Error>;}fn main(){}",
        ),
        (
            "multiple_forwarded_outputs",
            true,
            "trait Error{}trait Out{type A;type B:Error;}trait Root{type A;type B:Error;type Seq:Out<A=Self::A,B=Self::B>;}struct W<T>(T);impl<T:Root>Root for W<T>{type A=T::A;type B=T::B;type Seq=T::Seq;}fn main(){}",
        ),
        (
            "concrete_forwarded_outputs",
            true,
            "trait Error{}trait Out{type A;type B:Error;}trait Root{type A;type B:Error;type Seq:Out<A=Self::A,B=Self::B>;}struct Empty<A,E>(core::marker::PhantomData<(A,E)>);impl<A,E:Error>Out for Empty<A,E>{type A=A;type B=E;}struct W<T>(T);impl<T:Root>Root for W<T>{type A=T::A;type B=T::B;type Seq=Empty<T::A,T::B>;}fn main(){}",
        ),
        (
            "wrapped_forwarded_outputs",
            true,
            "trait Error{}trait Out{type A;type B:Error;}trait Root{type A;type B:Error;type Seq:Out<A=Self::A,B=Self::B>;}struct Wrap<S>(S);impl<S:Out>Out for Wrap<S>{type A=S::A;type B=S::B;}struct W<T>(T);impl<T:Root>Root for W<T>{type A=T::A;type B=T::B;type Seq=Wrap<T::Seq>;}fn main(){}",
        ),
        (
            "wrapped_wrong_output",
            false,
            "trait Error{}trait Out{type A;type B:Error;}trait Root{type A;type B:Error;type Seq:Out<A=Self::A,B=Self::B>;}struct Wrap<S>(S);impl<S:Out>Out for Wrap<S>{type A=u32;type B=S::B;}struct W<T>(T);impl<T:Root>Root for W<T>{type A=T::A;type B=T::B;type Seq=Wrap<T::Seq>;}fn main(){}",
        ),
        (
            "header_implies_supertrait_equality",
            true,
            "trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined;}trait Outer:Derived{}impl<T:Derived>Outer for T{}fn main(){}",
        ),
        (
            "header_cannot_imply_required_equality",
            false,
            "trait Base{type Out;}trait Derived:Base<Out=Self::Defined>{type Defined;}struct W<T>(T);impl<T:Base>Base for W<T>{type Out=T::Out;}impl<T:Base>Derived for W<T>{type Defined=u32;}fn main(){}",
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
