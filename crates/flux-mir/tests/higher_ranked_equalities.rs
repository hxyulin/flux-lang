//! Quantified associated outputs retain the lifetimes of their original inputs.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-ranked-equality-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    let source = format!("{source}\nfn main() {{}}\n");
    std::fs::write(&path, &source).unwrap();
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
    let (_, _, hir) = flux_hir::lower_text(&path, &source);
    let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}

#[test]
fn scalar() {
    compare(
        "scalar",
        r#"trait Get { type Item; fn get(self)->Self::Item; }
impl<'a> Get for &'a u32 { type Item = u32; fn get(self)->u32{*self} }
fn get<T>(x:&T)->u32 where for<'a> &'a T:Get<Item=u32>{x.get()}
"#,
        true,
    );
}

#[test]
fn borrowed_output() {
    compare(
        "borrowed_output",
        r#"trait Get {type Item; fn get(self)->Self::Item;} impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} fn f<T>(x:&T)->&u32 where for<'a> &'a T:Get<Item=&'a u32> {x.get()}"#,
        true,
    );
}

#[test]
fn borrowed_output_escape() {
    compare(
        "borrowed_output_escape",
        r#"trait Get {type Item; fn get(self)->Self::Item;} impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} fn f<T>(x:&T)->&'static u32 where for<'a> &'a T:Get<Item=&'a u32> {x.get()}"#,
        false,
    );
}

#[test]
fn iterator_items() {
    compare(
        "iterator_items",
        r#"fn pair<I: Iterator,J:Iterator<Item=I::Item>>(_:I,_:J){} fn f<L,R>(l:&L,r:&R) where for<'a> &'a L:IntoIterator,for<'a> &'a R:IntoIterator<Item=<&'a L as IntoIterator>::Item> {pair(l.into_iter(),r.into_iter());} fn invoke(){f(&vec![1u32],&vec![2u32]);}"#,
        true,
    );
}

#[test]
fn iterator_item_escape() {
    compare(
        "iterator_item_escape",
        r#"fn f<L:'static,R>(r:&R)->Option<<&'static L as IntoIterator>::Item> where for<'a> &'a L:IntoIterator,for<'a> &'a R:IntoIterator<Item=<&'a L as IntoIterator>::Item> {r.into_iter().next()}"#,
        false,
    );
}

#[test]
fn closure_output() {
    compare(
        "closure_output",
        r#"trait Get {type Item; fn get(self)->Self::Item;} impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} fn f<T>(x:&T)->&u32 where for<'a> &'a T:Get<Item=&'a u32> {(||x.get())()}"#,
        true,
    );
}

#[test]
fn closure_output_escape() {
    compare(
        "closure_output_escape",
        r#"trait Get {type Item; fn get(self)->Self::Item;} impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} fn f<T>(x:&T)->&'static u32 where for<'a> &'a T:Get<Item=&'a u32> {(||x.get())()}"#,
        false,
    );
}

#[test]
fn impl_header() {
    compare(
        "impl_header",
        r#"trait Get {type Item; fn get(self)->Self::Item;} impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} struct Wrapper<T>(T); impl<T> Wrapper<T> where for<'a> &'a T:Get<Item=&'a u32> {fn get(&self)->&u32 {(&self.0).get()}}"#,
        true,
    );
}

#[test]
fn static_output_cannot_satisfy_equality() {
    compare(
        "static_output_cannot_satisfy_equality",
        r#"trait Get {type Item; fn get(self)->Self::Item;} struct Static;static VALUE:u32=7;impl<'a> Get for &'a Static {type Item=&'static u32;fn get(self)->Self::Item{&VALUE}} fn need<T>(_:&T) where for<'a> &'a T:Get<Item=&'a u32> {} fn invoke(){need(&Static);}"#,
        false,
    );
}

#[test]
fn distinct_input_correlation() {
    compare(
        "distinct_input_correlation",
        r#"trait Combine<A>{type Item;fn get(self,a:A)->Self::Item;} impl<'a,'b> Combine<&'b u32> for &'a u32 {type Item=&'a u32;fn get(self,_:&'b u32)->Self::Item{self}} fn f<'x,'y,T>(x:&'x T,y:&'y u32)->&'x u32 where for<'a,'b> &'a T:Combine<&'b u32,Item=&'a u32>{x.get(y)}"#,
        true,
    );
}

#[test]
fn distinct_input_escape() {
    compare(
        "distinct_input_escape",
        r#"trait Combine<A>{type Item;fn get(self,a:A)->Self::Item;} impl<'a,'b> Combine<&'b u32> for &'a u32 {type Item=&'a u32;fn get(self,_:&'b u32)->Self::Item{self}} fn f<'x,'y,T>(x:&'x T,y:&'y u32)->&'y u32 where for<'a,'b> &'a T:Combine<&'b u32,Item=&'a u32>{x.get(y)}"#,
        false,
    );
}

#[test]
fn wrong_output_slot() {
    compare(
        "wrong_output_slot",
        r#"trait Combine<A>{type Item;fn get(self,a:A)->Self::Item;} impl<'a,'b> Combine<&'b u32> for &'a u32 {type Item=&'a u32;fn get(self,_:&'b u32)->Self::Item{self}} fn need<T>(_:&T) where for<'a,'b> &'a T:Combine<&'b u32,Item=&'b u32>{} fn invoke(){need(&3u32);}"#,
        false,
    );
}

#[test]
fn mutable_iterator_items() {
    compare(
        "mutable_iterator_items",
        r#"fn pair<I: Iterator,J:Iterator<Item=I::Item>>(_:I,_:J){} fn f<L,R>(l:&mut L,r:&mut R) where for<'a> &'a mut L:IntoIterator,for<'a> &'a mut R:IntoIterator<Item=<&'a mut L as IntoIterator>::Item>{pair(l.into_iter(),r.into_iter());} fn invoke(){f(&mut vec![1u32],&mut vec![2u32]);}"#,
        true,
    );
}

#[test]
fn trait_header() {
    compare(
        "trait_header",
        r#"trait Get {type Item;fn get(self)->Self::Item;}impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} trait Container where for<'a> &'a Self:Get<Item=&'a u32>{fn value(&self)->&u32{self.get()}} impl Container for u32{}"#,
        true,
    );
}

#[test]
fn trait_header_escape() {
    compare(
        "trait_header_escape",
        r#"trait Get {type Item;fn get(self)->Self::Item;}impl<'a> Get for &'a u32 {type Item=&'a u32;fn get(self)->Self::Item{self}} trait Container where for<'a> &'a Self:Get<Item=&'a u32>{fn value(&self)->&'static u32{self.get()}}"#,
        false,
    );
}

#[test]
fn trait_lifetime_input() {
    compare(
        "trait_lifetime_input",
        r#"trait View<'a>{type Item;fn view(&'a self)->Self::Item;}impl<'a> View<'a> for u32{type Item=&'a u32;fn view(&'a self)->Self::Item{self}} fn f<T>(x:&T)->&u32 where for<'a> T:View<'a,Item=&'a u32>{x.view()}"#,
        true,
    );
}

#[test]
fn trait_lifetime_escape() {
    compare(
        "trait_lifetime_escape",
        r#"trait View<'a>{type Item;fn view(&'a self)->Self::Item;}impl<'a> View<'a> for u32{type Item=&'a u32;fn view(&'a self)->Self::Item{self}} fn f<T>(x:&T)->&'static u32 where for<'a> T:View<'a,Item=&'a u32>{x.view()}"#,
        false,
    );
}

#[test]
fn opaque_lifetime_input() {
    compare(
        "opaque_lifetime_input",
        r#"trait View<'a>{type Item;fn view(&'a self)->Self::Item;}impl<'a> View<'a> for u32{type Item=&'a u32;fn view(&'a self)->Self::Item{self}} fn make()->impl for<'a> View<'a,Item=&'a u32>{42u32} fn f()->u32{let value=make();*value.view()}"#,
        true,
    );
}

#[test]
fn output_only_binder() {
    compare(
        "output_only_binder",
        r#"trait Get{type Item;} fn f<T>()where for<'a> T:Get<Item=&'a u32>{}"#,
        false,
    );
}

#[test]
fn opaque_lifetime_escape() {
    compare(
        "opaque_lifetime_escape",
        r#"trait View<'a>{type Item;fn view(&'a self)->Self::Item;}
impl<'a> View<'a> for u32{type Item=&'a u32;fn view(&'a self)->Self::Item{self}}
fn make()->impl for<'a> View<'a,Item=&'a u32>{42u32}
fn f()->&'static u32{let value=make();value.view()}"#,
        false,
    );
}

#[test]
fn opaque_output_only_binder() {
    compare(
        "opaque_output_only_binder",
        r#"trait Get{type Item;} struct Value;
impl Get for Value{type Item=&'static u32;}
fn make()->impl for<'a> Get<Item=&'a u32>{Value}"#,
        false,
    );
}

#[test]
fn overlapping() {
    compare(
        "overlapping",
        r#"trait Pair<A>{type Out;fn get(self,a:A)->Self::Out;} fn f<'a,'b,T>(x:&'a T,y:&'b u32)->&'a u32 where for<'x> &'x T:Pair<&'x u32,Out=&'x u32>,for<'x,'y> &'x T:Pair<&'y u32,Out=&'x u32>{x.get(y)} "#,
        false,
    );
}

#[test]
fn duplicate() {
    compare(
        "duplicate",
        r#"trait Pair<A>{type Out;fn get(self,a:A)->Self::Out;} fn f<'a,T>(x:&'a T,y:&'a u32)->&'a u32 where for<'x> &'x T:Pair<&'x u32,Out=&'x u32>,for<'y> &'y T:Pair<&'y u32,Out=&'y u32>{x.get(y)} "#,
        true,
    );
}

#[test]
fn static_and_universal() {
    compare(
        "static_and_universal",
        r#"trait Get{type Item;fn get(self)->Self::Item;}fn f<T:'static>(x:&T)->&u32 where &'static T:Get<Item=&'static u32>, for<'a> &'a T:Get<Item=&'a u32>{x.get()}"#,
        false,
    );
}

#[test]
fn opaque_static_output() {
    compare(
        "opaque_static_output",
        r#"trait View<'a>{type Item;fn view(&'a self)->Self::Item;}
struct Value;static N:u32=1;
impl<'a> View<'a> for Value{type Item=&'static u32;fn view(&'a self)->Self::Item{&N}}
fn make()->impl for<'a> View<'a,Item=&'a u32>{Value}"#,
        false,
    );
}

#[test]
fn possible_impl_must_not_normalize() {
    compare(
        "possible_impl_must_not_normalize",
        r#"trait Value{type Output;}struct Wrap<T>(T);impl<T:Clone> Value for Wrap<T>{type Output=u32;}
fn f<T>() -> <Wrap<T> as Value>::Output where Wrap<T>:Value {0u32}

"#,
        false,
    );
}

#[test]
fn definite_conditional_impl() {
    compare(
        "definite_conditional_impl",
        r#"trait Value{type Output;}struct Wrap<T>(T);impl<T:Clone> Value for Wrap<T>{type Output=u32;} fn f()-><Wrap<u32> as Value>::Output {42u32}"#,
        true,
    );
}

#[test]
fn definite_unconditional_impl() {
    compare(
        "definite_unconditional_impl",
        r#"trait Value{type Output;}struct Wrap<T:?Sized>(*const T);impl<T:?Sized> Value for Wrap<T>{type Output=u32;} fn f<T:?Sized>()-><Wrap<T> as Value>::Output {42u32}"#,
        true,
    );
}

#[test]
fn conflicting_output_rows() {
    compare(
        "conflicting_output_rows",
        r#"trait Get{type Item;fn get(self)->Self::Item;}
fn f<T>(x:&T)->&u32 where for<'a> &'a T:Get<Item=&'a u32>,for<'b> &'b T:Get<Item=&'static u32>{x.get()}"#,
        false,
    );
}

#[test]
fn reversed_binder_order() {
    compare(
        "reversed_binder_order",
        r#"trait Get<A>{type Item;fn get(self,a:A)->Self::Item;}
fn f<'a,'b,T>(x:&'a T,y:&'b u32)->&'a u32 where for<'x,'y> &'x T:Get<&'y u32,Item=&'x u32>,for<'y,'x> &'x T:Get<&'y u32,Item=&'x u32>{x.get(y)}"#,
        true,
    );
}

#[test]
fn builtin_type_contracts_remain_valid() {
    let hir = flux_hir::Core::get().hir();
    let typed = flux_ty::check_file_all(hir);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
}
