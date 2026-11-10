//! Caller proofs retain associated outputs and their lifetime rows.

fn compare(name: &str, source: &str, accepted: bool) {
    let root =
        std::env::temp_dir().join(format!("flux-rayon-solver-{name}-{}", std::process::id()));
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
fn inherited_projection_rows() {
    compare(
        "inherited_projection_rows",
        r#"trait Consumer<T>{type Result;}trait Unindexed<T>:Consumer<T>{}
trait Par:Sized{type Item;
 fn drive<C:Unindexed<Self::Item>>(self,c:C)->C::Result;
 fn map<F:Fn(Self::Item)->R+Send+Sync,R:Send>(self,f:F)->Map<Self,F>{Map(self,f)}
}
struct Map<I,F>(I,F);
impl<I:Par,F:Fn(I::Item)->R+Send+Sync,R:Send>Par for Map<I,F>{type Item=R;fn drive<C:Unindexed<R>>(self,_:C)->C::Result{loop{}}}
struct Words<'a>(&'a str);
impl<'a>Par for Words<'a>{type Item=&'a str;fn drive<C:Unindexed<Self::Item>>(self,_:C)->C::Result{loop{}}}
fn trim(s:&str)->&str{s}
fn forward<'a,C:Unindexed<&'a str>>(s:&'a str,c:C)->C::Result{Words(s).map(trim).drive(c)}
struct Lines<'a>(&'a str);impl<'a>Par for Lines<'a>{type Item=&'a str;fn drive<C:Unindexed<Self::Item>>(self,c:C)->C::Result{Words(self.0).map(trim).drive(c)}}

"#,
        true,
    );
}

#[test]
fn borrowed_callback() {
    compare(
        "borrowed_callback",
        r#"trait Producer { type Item; }
trait Callback<A> { type Output; fn call<P:Producer<Item=A>>(self,p:P)->Self::Output; }
trait Indexed { type Item; fn with<C:Callback<Self::Item>>(self,c:C)->C::Output; }
struct Cloned<I>(I);
struct Adapter<C>(C);
struct ClonedProducer<P>(P);
impl<'a,T:Clone+'a,P:Producer<Item=&'a T>> Producer for ClonedProducer<P>{type Item=T;}
impl<'a,T:Clone+'a,C:Callback<T>> Callback<&'a T> for Adapter<C>{
 type Output=C::Output;
 fn call<P:Producer<Item=&'a T>>(self,p:P)->Self::Output{self.0.call(ClonedProducer(p))}
}
impl<'a,T:Clone+'a,I:Indexed<Item=&'a T>> Indexed for Cloned<I>{
 type Item=T;
 fn with<C:Callback<T>>(self,c:C)->C::Output{self.0.with(Adapter(c))}
}

"#,
        true,
    );
}

#[test]
fn inherited_callable_output() {
    compare(
        "inherited_callable_output",
        r#"trait Consumer<T>{type Out;}
trait Par{type Item:Send;fn drive<C:Consumer<Self::Item>>(self,c:C)->C::Out;}
trait Indexed:Par{fn drive2<C:Consumer<Self::Item>>(self,c:C)->C::Out;}
struct Map<I,F>(I,F);
fn need<C:Consumer<T>,T>(_:C)->C::Out{loop{}}
impl<I:Par,F:Fn(I::Item)->R,R:Send>Par for Map<I,F>{type Item=F::Output;fn drive<C:Consumer<Self::Item>>(self,c:C)->C::Out{need::<C,R>(c)}}
impl<I:Indexed,F:Fn(I::Item)->R,R:Send>Indexed for Map<I,F>{fn drive2<C:Consumer<Self::Item>>(self,c:C)->C::Out{need::<C,R>(c)}}

"#,
        true,
    );
}

#[test]
fn compound_caller_output() {
    compare(
        "compound_caller_output",
        r#"trait Par{type Item:Send;}
trait IntoPar{type Item:Send;type Iter:Par<Item=Self::Item>;}
struct Iter<T>(T);struct Range<T>(T);
impl<T:Send>Par for Iter<T>{type Item=T;}
impl<T>IntoPar for Range<T>where Iter<T>:Par{type Item=<Iter<T>as Par>::Item;type Iter=Iter<T>;}

"#,
        true,
    );
}

#[test]
fn opaque_callback_iterator_rows() {
    compare(
        "opaque_callback_iterator_rows",
        r#"fn offset<T>(base:usize)->impl Fn((usize,T))->(usize,T){move|(i,x)|(base+i,x)}
fn consume<'a,I:IntoIterator<Item=(usize,&'a str)>>(_:I){}
fn f<'a>(s:&'a str){consume(s.match_indices('x').map(offset(1)));}

"#,
        true,
    );
}

#[test]
fn chain_caller_result_send() {
    compare(
        "chain_caller_result_send",
        r#"trait Consumer<T>:Send{type Result:Send;fn split(self)->(Self,Self)where Self:Sized;}
trait Par:Sized{type Item;fn drive<C:Consumer<Self::Item>>(self,c:C)->C::Result;}
trait Indexed:Par{fn drive2<C:Consumer<Self::Item>>(self,c:C)->C::Result;}
struct Chain<A,B>(A,B);
fn join<A:FnOnce()->R+Send,B:FnOnce()->S+Send,R:Send,S:Send>(a:A,b:B)->(R,S){(a(),b())}
impl<A:Par,B:Par<Item=A::Item>>Par for Chain<A,B>{type Item=A::Item;fn drive<C:Consumer<Self::Item>>(self,_:C)->C::Result{loop{}}}
impl<A:Indexed+Send,B:Indexed<Item=A::Item>+Send>Indexed for Chain<A,B>{fn drive2<C:Consumer<Self::Item>>(self,c:C)->C::Result{let Chain(a,b)=self;let(l,r)=c.split();let(x,_)=join(||a.drive(l),||b.drive(r));x}}

"#,
        true,
    );
}

#[test]
fn nested_output_only_impl_parameter() {
    compare(
        "nested_output_only_impl_parameter",
        r#"trait Consumer<T>{type Reducer;}
trait Unindexed<T>:Consumer<T>{fn reducer(&self)->Self::Reducer;}
struct Wrapper<C,F>(C,F);
impl<T,U,C:Consumer<U>,F:Fn(T)->Option<U>>Consumer<T>for Wrapper<C,F>{type Reducer=C::Reducer;}
impl<T,U,C:Unindexed<U>,F:Fn(T)->Option<U>>Unindexed<T>for Wrapper<C,F>{fn reducer(&self)->Self::Reducer{self.0.reducer()}}

"#,
        true,
    );
}

#[test]
fn mutable_capture_is_not_clone() {
    compare(
        "mutable_capture_is_not_clone",
        r#"fn need<T:Clone>(_:T){}fn f(x:&mut u32){need(move||*x+=1);}"#,
        false,
    );
}
#[test]
fn opaque_iterator_cannot_extend_lifetime() {
    compare(
        "opaque_iterator_cannot_extend_lifetime",
        r#"fn offset<T>(base:usize)->impl Fn((usize,T))->(usize,T){move|(i,x)|(base+i,x)}fn require<I:IntoIterator<Item=(usize,&'static str)>>(_:I){}fn f(s:&str){require(s.match_indices('x').map(offset(1)));}"#,
        false,
    );
}

#[test]
fn cloned_producer() {
    compare(
        "cloned_producer",
        r#"trait Producer{type Item;type IntoIter:Iterator<Item=Self::Item>+DoubleEndedIterator+ExactSizeIterator;}
struct Cloned<P>(P);
impl<'a,T:Clone+'a,P:Producer<Item=&'a T>>Producer for Cloned<P>{type Item=T;type IntoIter=std::iter::Cloned<P::IntoIter>;}

"#,
        true,
    );
}

#[test]
fn borrowed_iterator_conversion() {
    compare(
        "borrowed_iterator_conversion",
        r#"trait IntoPar{type Item;type Iter:Iterator<Item=Self::Item>;}
trait Ref<'a>{type Item:'a;type Iter:Iterator<Item=Self::Item>;}
impl<'a,I:'a+?Sized>Ref<'a>for I where &'a I:IntoPar{type Item=<&'a I as IntoPar>::Item;type Iter=<&'a I as IntoPar>::Iter;}

"#,
        true,
    );
}

#[test]
fn borrowed_projection_cannot_be_static() {
    compare(
        "borrowed_projection_cannot_be_static",
        r#"trait IntoPar{type Item;}trait Ref<'a>{type Item:'static;}impl<'a,I:'a+?Sized>Ref<'a>for I where &'a I:IntoPar{type Item=<&'a I as IntoPar>::Item;}"#,
        false,
    );
}

#[test]
fn later_callable_output_bound() {
    compare(
        "later_callable_output_bound",
        r#"trait Folder<T>{type Result;}
trait Consumer<T>:Send{type Folder:Folder<T,Result=Self::Result>;type Result:Send;}
struct MC<'a,C,F>(C,&'a F);struct MF<'a,C,F>(C,&'a F);
impl<'a,T,C:Consumer<F::Output>,F:Fn(T)->R+Sync,R:Send>Consumer<T>for MC<'a,C,F>{type Folder=MF<'a,C::Folder,F>;type Result=C::Result;}
impl<'a,T,R,C:Folder<R>,F:Fn(T)->R>Folder<T>for MF<'a,C,F>{type Result=C::Result;}

"#,
        true,
    );
}

#[test]
fn borrowed_multizip() {
    compare(
        "borrowed_multizip",
        r#"trait Par:Send+Sized{type Item:Send;}
trait Indexed:Par{}
trait Ref<'a>{type Item;type Iter:Par<Item=Self::Item>;}
trait IntoPar{type Item;type Iter:Par<Item=Self::Item>;}
struct Zip<A>(A);
impl<A:Indexed>Par for Zip<A>{type Item=A::Item;}
impl<'a,A:Ref<'a,Iter:Indexed>>IntoPar for &'a(A,){type Item=A::Item;type Iter=Zip<A::Iter>;}

"#,
        true,
    );
}

#[test]
fn clone_capture_cannot_hide_static_condition() {
    compare(
        "clone_capture_cannot_hide_static_condition",
        r#"
struct Guard<T>(T);impl<T:Copy+'static>Clone for Guard<T>{fn clone(&self)->Self{Guard(self.0)}}
impl<T:Copy+'static>Copy for Guard<T>{}
fn require<T:Clone>(_:T){}
fn borrowed<'a>(x:&'a u32){let guard=Guard(x);require(move||{let _=&guard;});}
"#,
        false,
    );
}
