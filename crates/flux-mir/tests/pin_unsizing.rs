//! Pinned pointer coercions retain pointer capabilities and lifetime obligations.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn pinned_unsizing_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-pin-unsizing-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "discarded_pin_lifetime",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn f(p:Pin<&mut u32>){let _:Pin<&'static mut dyn Tr>=p;}fn main(){}",
            false,
        ),
        (
            "discarded_reference_lifetime",
            "fn f(){let a=[1,2];let _:&'static [i32]=&a;}fn main(){}",
            false,
        ),
        (
            "associated_region_invariance",
            "use core::{pin::Pin,future::Future};fn f<'a:'b,'b,F:Future<Output=&'a u32>+'a>(v:F)->Pin<Box<dyn Future<Output=&'b u32>+'b>>{Box::pin(v)}fn main(){}",
            false,
        ),
        (
            "associated_region_equivalence",
            "use core::{pin::Pin,future::Future};fn f<'a:'b,'b:'a,F:Future<Output=&'a u32>+'a>(v:F)->Pin<Box<dyn Future<Output=&'b u32>+'b>>{Box::pin(v)}fn main(){}",
            true,
        ),
        (
            "constructor_requires_unpin",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn main(){let mut n=7u32;let _:Pin<&mut dyn Tr>=Pin::new(&mut n);}",
            false,
        ),
        (
            "boxed_new_requires_unpin",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn main(){let _:Pin<Box<dyn Tr>>=Pin::new(Box::new(7u32));}",
            false,
        ),
        (
            "consumed_reference_pin",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn f(p:Pin<&mut u32>){let _:Pin<&mut dyn Tr>=p;let _=p.as_ref();}fn main(){}",
            false,
        ),
        (
            "object_no_send_strengthening",
            "use core::{pin::Pin,future::Future};fn f(p:Pin<Box<dyn Future<Output=u32>>>)->Pin<Box<dyn Future<Output=u32>+Send>>{p}fn main(){}",
            false,
        ),
        (
            "generic_borrowed_object",
            "fn f<'a,T:Iterator<Item=&'a u32>+'a>(v:T)->Box<dyn Iterator<Item=&'a u32>+'a>{Box::new(v)}fn main(){}",
            true,
        ),
        (
            "generic_borrowed_object_invariance",
            "fn f<'a:'b,'b,T:Iterator<Item=&'a u32>+'a>(v:T)->Box<dyn Iterator<Item=&'b u32>+'b>{Box::new(v)}fn main(){}",
            false,
        ),
        (
            "boxed_future",
            "use core::{pin::Pin,future::Future};fn f<T:Future+'static>(v:T)->Pin<Box<dyn Future<Output=T::Output>>>{Box::pin(v)}fn main(){}",
            true,
        ),
        (
            "boxed_empty_slice",
            "use core::pin::Pin;fn f<T>()->Pin<Box<[T]>>{Box::pin([])}fn main(){}",
            true,
        ),
        (
            "mutable_to_shared",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn f(p:Pin<&mut u32>)->Pin<&dyn Tr>{p}fn main(){}",
            true,
        ),
        (
            "no_deref_coercion",
            "use core::pin::Pin;fn f(p:Pin<&String>)->Pin<&str>{p}fn main(){}",
            false,
        ),
        (
            "borrowed_output",
            "use core::{pin::Pin,future::Future};fn f<'a,T:Future<Output=&'a u32>+'a>(v:T)->Pin<Box<dyn Future<Output=&'a u32>+'a>>{Box::pin(v)}fn main(){}",
            true,
        ),
        (
            "borrowed_output_escape",
            "use core::{pin::Pin,future::Future};fn f<'a,T:Future<Output=&'a u32>+'a>(v:T)->Pin<Box<dyn Future<Output=&'a u32>+'static>>{Box::pin(v)}fn main(){}",
            false,
        ),
        (
            "wrong_associated_output",
            "use core::{pin::Pin,future::Future};fn f<T:Future<Output=u32>+'static>(v:T)->Pin<Box<dyn Future<Output=u64>>>{Box::pin(v)}fn main(){}",
            false,
        ),
        (
            "missing_send",
            "use core::{pin::Pin,future::Future};fn f<T:Future+'static>(v:T)->Pin<Box<dyn Future<Output=T::Output>+Send>>{Box::pin(v)}fn main(){}",
            false,
        ),
        (
            "send_bound",
            "use core::{pin::Pin,future::Future};fn f<T:Future+Send+'static>(v:T)->Pin<Box<dyn Future<Output=T::Output>+Send>>{Box::pin(v)}fn main(){}",
            true,
        ),
        (
            "no_unpin",
            "use core::pin::Pin;trait Tr{}struct S(core::marker::PhantomPinned);impl Tr for S{}fn f(p:Pin<Box<S>>)->Pin<Box<dyn Tr+Unpin>>{p}fn main(){}",
            false,
        ),
        (
            "ref_escape",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn f<'a>(p:Pin<&'a mut u32>)->Pin<&'static mut dyn Tr>{p}fn main(){}",
            false,
        ),
        (
            "ref_mut_strengthening",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn f(p:Pin<&u32>)->Pin<&mut dyn Tr>{p}fn main(){}",
            false,
        ),
        (
            "reference_owner_escape",
            "use core::pin::Pin;trait Tr{}impl Tr for u32{}fn main(){let p;{let mut x=1;p=Pin::new(&mut x);}let _:Pin<&mut dyn Tr>=p;}",
            false,
        ),
        (
            "array_element_mismatch",
            "use core::pin::Pin;fn f(p:Pin<Box<[u32;3]>>)->Pin<Box<[u64]>>{p}fn main(){}",
            false,
        ),
        (
            "tail_slice",
            "use core::pin::Pin;struct Tail<T:?Sized>{tag:u32,data:T}fn f(p:Pin<Box<Tail<[u32;3]>>>)->Pin<Box<Tail<[u32]>>>{p}fn main(){}",
            true,
        ),
        (
            "shared_arc",
            "use core::pin::Pin;use std::sync::Arc;trait Tr{}impl Tr for u32{}fn f(p:Pin<Arc<u32>>)->Pin<Arc<dyn Tr>>{p}fn main(){}",
            true,
        ),
        (
            "shared_rc_slice",
            "use core::pin::Pin;use std::rc::Rc;fn f(p:Pin<Rc<[u32;3]>>)->Pin<Rc<[u32]>>{p}fn main(){}",
            true,
        ),
        (
            "no_custom_pin",
            "struct Pin<T>(T);trait Tr{}impl Tr for u32{}fn f(p:Pin<Box<u32>>)->Pin<Box<dyn Tr>>{p}fn main(){}",
            false,
        ),
        (
            "no_owner_switch",
            "use core::pin::Pin;use std::rc::Rc;fn f(p:Pin<Box<[u32;3]>>)->Pin<Rc<[u32]>>{p}fn main(){}",
            false,
        ),
        (
            "object_upcast",
            "use core::pin::Pin;trait Base{}trait Sub:Base{}fn f(p:Pin<Box<dyn Sub>>)->Pin<Box<dyn Base>>{p}fn main(){}",
            true,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.clone());
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
