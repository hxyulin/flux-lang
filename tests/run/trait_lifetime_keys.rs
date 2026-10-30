struct AmbiguousObject<'a,'b,T:?Sized+'a+'b>{first:&'a (),second:&'b (),value:Box<T>}
struct ObjectHolder<'a,T:?Sized+'a> {marker:&'a (),value:Box<T>}
type BoundObject<'a,T:?Sized+'a> = ObjectHolder<'a,T>;
fn bounded_object<'a>(marker:&'a (),value:&'a str)->Box<dyn core::fmt::Display+'a> {
    let holder:ObjectHolder<'a,dyn core::fmt::Display>=ObjectHolder{marker,value:Box::new(value)};
    holder.value
}
fn bounded_alias<'a>(marker:&'a (),value:&'a str)->Box<dyn core::fmt::Display+'a> {
    let holder:BoundObject<'a,dyn core::fmt::Display>=ObjectHolder{marker,value:Box::new(value)};
    holder.value
}
struct Nodes<N,const CAP:usize> {nodes:[Option<(N,u32)>;CAP],len:usize}
impl<N:Copy,const CAP:usize> Nodes<N,CAP> {
    fn iter(&self)->impl Iterator<Item=(N,u32)>+'_ {
        self.nodes[..self.len].iter().flatten().copied()
    }
}
fn local_object(text:&str) {
    let display:Box<dyn core::fmt::Display>=Box::new(text);
    assert_eq!(format!("{}",display),text);
}
fn erased_writer<W:std::io::Write>(writer:W)->Box<dyn std::io::Write> {
    // SAFETY: callers in this fixture destroy the object before its borrowed writer.
    // This deliberately mirrors the backend dependency's lifetime-erasure adapter.
    unsafe {core::mem::transmute::<Box<dyn std::io::Write>,Box<dyn std::io::Write>>(Box::new(writer))}
}
trait LocalAny {
    fn id(&self)->core::any::TypeId where Self:'static;
}
impl<T:?Sized> LocalAny for core::marker::PhantomData<T> {
    fn id(&self)->core::any::TypeId where Self:'static {core::any::TypeId::of::<T>()}
}
struct StaticView<'a>(&'a i32);
impl StaticView<'_> {
    fn get(self)->&'static i32 where Self:'static {self.0}
}
static STATIC_NUMBER:i32=37;
use std::borrow::Borrow;
trait Keys<'a,K:'a>:Copy {
    fn len(self)->usize;
    unsafe fn get(self,index:usize)->&'a K;
    fn first(self,count:usize)->Self;
    fn position(self,compare:impl Fn(&K)->std::cmp::Ordering)->Option<usize> {
        let mut index=0;
        while index<self.len() {
            // SAFETY: the index is bounded by this view's length.
            if compare(unsafe {self.get(index)})==std::cmp::Ordering::Equal {return Some(index);}
            index+=1;
        }
        None
    }
    fn contains<Q:Ord+?Sized>(self,key:&Q)->bool where K:Borrow<Q> {
        let mut index=0;
        while index<self.len() {
            // SAFETY: the index is bounded by this view's length.
            if unsafe {self.get(index)}.borrow()==key {return true;}
            index+=1;
        }
        false
    }
}
struct View<'a,K>(&'a [K]);
impl<K> Copy for View<'_,K> {}
impl<K> Clone for View<'_,K> {fn clone(&self)->Self {*self}}
impl<'a,K> Keys<'a,K> for View<'a,K> {
    fn len(self)->usize {self.0.len()}
    unsafe fn get(self,index:usize)->&'a K {&self.0[index]}
    fn first(self,count:usize)->Self {Self(&self.0[..count])}
}
fn first<'a,K:'a,L:Keys<'a,K>>(keys:L)->&'a K {
    assert!(keys.len()>0);
    // SAFETY: the view has at least one key.
    unsafe {keys.get(0)}
}
fn contains<'a,K:'a,L:Keys<'a,K>,Q:Ord+?Sized>(keys:L,key:&Q)->bool where K:Borrow<Q> {
    keys.contains(key)
}
fn main() {
    let marker=();
    let ambiguous:AmbiguousObject<'_,'_,dyn core::fmt::Display>=AmbiguousObject{first:&marker,second:&marker,value:Box::new(7)};
    assert_eq!(format!("{}",ambiguous.value),"7");
    let value=String::from("bounded");
    assert_eq!(format!("{}",bounded_object(&marker,value.as_str())),"bounded");
    assert_eq!(format!("{}",bounded_alias(&marker,value.as_str())),"bounded");
    let left=11;
    let right=17;
    let nodes=Nodes{nodes:[Some((&left,2)),None,Some((&right,3))],len:3};
    assert_eq!(nodes.iter().map(|(value,version)|*value as u32+version).sum::<u32>(),33);
    let text=String::from("borrowed object");
    local_object(text.as_str());
    let mut written=Vec::new();
    {let mut writer=erased_writer(&mut written);writer.write_all(b"ok").unwrap();}
    assert_eq!(written,b"ok");
    assert_eq!(core::marker::PhantomData::<u32>.id(),core::any::TypeId::of::<u32>());
    assert_eq!(core::marker::PhantomData::<str>.id(),core::any::TypeId::of::<str>());
    assert_eq!(*StaticView(&STATIC_NUMBER).get(),37);
    let keys=[String::from("alpha"),String::from("beta"),String::from("gamma")];
    let view=View(&keys);
    assert_eq!(first(view),"alpha");
    assert!(contains(view,"gamma"));
    assert!(!contains(view.first(2),"gamma"));
    assert_eq!(view.len(),3);
    let target=String::from("beta");
    assert_eq!(view.position(|key|key.cmp(&target)),Some(1));
    assert_eq!(view.first(1).position(|key|key.cmp(&target)),None);
    let number=19;
    let references=[&number];
    let view=View(&references);
    assert_eq!(**first(view),19);
    println!("{} {}",first(View(&keys)),**first(view));
}
