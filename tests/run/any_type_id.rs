use std::any::{Any, TypeId};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
#[repr(align(128))]
struct Owner(u32);
impl Drop for Owner {
    fn drop(&mut self) {
        DROPS.fetch_add(self.0 as usize, Ordering::SeqCst);
    }
}
mod left {
    pub struct Same;
}
mod right {
    pub struct Same;
}
type Alias = left::Same;
trait Project {
    type Output;
}
impl Project for left::Same {
    type Output = u32;
}
fn captured<T: 'static>() -> TypeId {
    TypeId::of::<fn(T)>()
}
trait Marked: Send {}
const INTEGER: TypeId = TypeId::of::<u32>();
fn static_free<'a: 'static>() -> TypeId {
    TypeId::of::<fn(&'a u32)>()
}
trait Inherited: Any + Send + Sync {
    fn value(&self) -> u32;
}
impl Inherited for u32 {
    fn value(&self) -> u32 {
        *self
    }
}
fn inherited(x: &dyn Inherited) -> &(dyn Any + Send + Sync) {
    x
}
fn inherited_arc(x: Arc<dyn Inherited>) -> Arc<dyn Any + Send + Sync> {
    x
}
struct TypeIdHasher { calls:usize }
impl std::hash::Hasher for TypeIdHasher {
    fn write(&mut self,_:&[u8]){panic!("TypeId must use write_u64");}
    fn write_u64(&mut self,_:u64){self.calls+=1;}
    fn finish(&self)->u64{self.calls as u64}
}
fn main() {
    use std::hash::{Hash,Hasher};
    let mut hasher=TypeIdHasher{calls:0};
    TypeId::of::<u32>().hash(&mut hasher);
    assert_eq!(hasher.finish(),1);
    let a: &dyn Inherited = &41u32;
    assert_eq!(inherited(a).downcast_ref::<u32>(), Some(&41));
    let marker: &dyn Send = a;
    assert_eq!(std::mem::size_of_val(marker), 4);
    let owner: Arc<dyn Inherited> = Arc::new(43u32);
    let marker: Arc<dyn Send + Sync> = owner.clone();
    assert_eq!(std::mem::size_of_val(&*marker), 4);
    assert_eq!(*inherited_arc(owner).downcast::<u32>().ok().unwrap(), 43);
    assert_eq!(INTEGER, TypeId::of::<u32>());
    assert_ne!(INTEGER, TypeId::of::<i32>());
    assert_ne!(TypeId::of::<left::Same>(), TypeId::of::<right::Same>());
    assert_eq!(TypeId::of::<Alias>(), TypeId::of::<left::Same>());
    assert_eq!(TypeId::of::<for<'a> fn()>(), TypeId::of::<fn()>());
    assert_eq!(
        TypeId::of::<for<'a> fn(&'a u32)>(),
        TypeId::of::<for<'b> fn(&'b u32)>()
    );
    assert_eq!(static_free(), TypeId::of::<fn(&'static u32)>());
    assert_ne!(TypeId::of::<fn(&'static u32)>(), TypeId::of::<fn(&u32)>());
    assert_eq!(
        TypeId::of::<dyn Any + Send + Sync>(),
        TypeId::of::<dyn Sync + Any + Send>()
    );
    assert_ne!(TypeId::of::<dyn Any>(), TypeId::of::<dyn Any + Send>());
    assert_eq!(
        TypeId::of::<dyn Send + Sync>(),
        TypeId::of::<dyn Sync + Send>()
    );
    assert_ne!(TypeId::of::<[u8; 2]>(), TypeId::of::<[u8; 3]>());
    let mut ids = vec![TypeId::of::<u32>(), TypeId::of::<u8>(), TypeId::of::<u32>()];
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 2);
    let mut set = std::collections::HashSet::new();
    set.insert(INTEGER);
    assert!(set.contains(&TypeId::of::<u32>()));
    assert!(!set.contains(&TypeId::of::<u8>()));
    assert!(format!("{:?}", INTEGER).starts_with("TypeId(0x"));
    assert_eq!(TypeId::of::<<left::Same as Project>::Output>(), INTEGER);
    assert_eq!(captured::<&'static u32>(), TypeId::of::<fn(&'static u32)>());
    assert_eq!(
        TypeId::of::<for<'a, 'b> fn(&'a u32, &'b u32)>(),
        TypeId::of::<for<'x, 'y> fn(&'y u32, &'x u32)>()
    );
    assert_ne!(
        TypeId::of::<for<'a, 'b> fn(&'a u32, &'b u32) -> &'a u32>(),
        TypeId::of::<for<'a, 'b> fn(&'a u32, &'b u32) -> &'b u32>()
    );
    assert_ne!(
        TypeId::of::<dyn Marked>(),
        TypeId::of::<dyn Marked + Send>()
    );
    let mut text = String::from("hello");
    let value: &mut dyn Any = &mut text;
    assert!(value.is::<String>());
    assert!(!value.is::<u32>());
    assert!(value.downcast_ref::<u32>().is_none());
    value.downcast_mut::<String>().unwrap().push_str(" world");
    assert_eq!(value.downcast_ref::<String>().unwrap(), "hello world");
    let marked: &(dyn Any + Send + Sync) = &7u32;
    assert_eq!(marked.downcast_ref::<u32>(), Some(&7));
    let owner: Arc<dyn Any + Send + Sync> = Arc::new(String::from("arc"));
    let weak = Arc::downgrade(&owner);
    let clone = owner.clone();
    let failed = owner.downcast::<u32>().unwrap_err();
    assert_eq!(Arc::strong_count(&failed), 2);
    let typed = failed.downcast::<String>().ok().unwrap();
    assert_eq!(&**typed, "arc");
    assert_eq!(Arc::strong_count(&typed), 2);
    drop(clone);
    assert_eq!(
        weak.upgrade().unwrap().downcast_ref::<String>().unwrap(),
        "arc"
    );
    drop(typed);
    assert!(weak.upgrade().is_none());
    let boxed: Box<dyn Any + Send> = Box::new(12u32);
    assert_eq!(boxed.downcast_ref::<u32>(), Some(&12));
    assert_eq!(format!("{:?}", boxed), "Any { .. }");
    let own: Arc<dyn Any + Send + Sync> = Arc::new(Owner(3));
    let address = &*own as *const (dyn Any + Send + Sync) as *const ();
    let other = own.clone();
    let own = own.downcast::<u64>().unwrap_err();
    assert!(Arc::ptr_eq(&own, &other));
    let own = own.downcast::<Owner>().ok().unwrap();
    assert_eq!(&*own as *const Owner as *const (), address);
    drop(other);
    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    drop(own);
    assert_eq!(DROPS.load(Ordering::SeqCst), 3);
    println!(
        "identity {} {} {}",
        text,
        INTEGER == TypeId::of::<u32>(),
        DROPS.load(Ordering::SeqCst)
    );
}
