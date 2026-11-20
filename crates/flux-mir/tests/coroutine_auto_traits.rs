//! Coroutine auto traits describe stored state, independently of output type.
#[test]
fn send_sync_storage_matches_rustc() {
    let prelude = "use std::future::{Future,ready};use std::rc::Rc;use std::cell::Cell;fn send<F:Future+Send>(_:F){}fn sync<F:Future+Sync>(_:F){}";
    let deep_type = (0..12).fold("T".to_owned(), |inner, _| format!("({inner},)"));
    let deep_value = (0..12).fold("x".to_owned(), |inner, _| format!("({inner},)"));
    let deep_send = format!(
        "async fn acquire<T>(x:{deep_type}){{ready(()).await;drop(x)}}fn check<T:Send>(x:T){{send(acquire({deep_value}));}}fn main(){{check(7u32)}}"
    );
    let deep_static = format!(
        "struct Static<T>(T);unsafe impl<T:'static>Send for Static<T>{{}}async fn acquire<T>(x:{deep_type}){{ready(()).await;drop(x)}}fn check<'a>(p:&'a u32){{let x=Static(p);send(acquire({deep_value}));}}fn main(){{}}"
    );
    let cases = [
        ("deep_structural_send", deep_send.as_str(), true),
        ("deep_selected_static_bound", deep_static.as_str(), false),
        ("empty", "fn main(){send(async {});}", true),
        ("rc_output", "fn main(){send(async {Rc::new(7)});}", true),
        (
            "raw_output",
            "fn main(){send(async {std::ptr::null::<u32>()});}",
            true,
        ),
        (
            "dead_rc",
            "fn main(){send(async {{let x=Rc::new(7);let _=*x;}ready(()).await;});}",
            true,
        ),
        (
            "held_rc",
            "fn main(){send(async {let x=Rc::new(7);ready(()).await;*x});}",
            false,
        ),
        (
            "drop_only_rc",
            "fn main(){send(async {let _held=Rc::new(7);ready(()).await;});}",
            false,
        ),
        (
            "held_raw",
            "fn main(){send(async {let x=std::ptr::null::<u32>();ready(()).await;x});}",
            false,
        ),
        (
            "captured_rc_without_await",
            "fn main(){let x=Rc::new(7);send(async move{drop(x)});}",
            false,
        ),
        (
            "generic_capture",
            "fn make<T:Send>(x:T){send(async move{ready(()).await;drop(x)});}fn main(){make(7);}",
            true,
        ),
        (
            "generic_non_send_output",
            "async fn make<T:Default>()->T{T::default()}fn main(){send(make::<Rc<u32>>());}",
            true,
        ),
        (
            "generic_spill",
            "async fn make<T:Default>()->T{let x=T::default();ready(()).await;x}fn main(){send(make::<u32>());}",
            true,
        ),
        (
            "generic_non_send_spill",
            "async fn make<T:Default>()->T{let x=T::default();ready(()).await;x}fn main(){send(make::<Rc<u32>>());}",
            false,
        ),
        (
            "associated_storage_equality",
            "trait Family{type Item;fn make()->Self::Item;}async fn stored<T:Family>(){let value=T::make();ready(()).await;drop(value);}fn check<T:Family<Item=u32>>(){send(stored::<T>());}struct Marker;impl Family for Marker{type Item=u32;fn make()->u32{7}}fn main(){check::<Marker>();}",
            true,
        ),
        (
            "associated_non_send_storage",
            "trait Family{type Item;fn make()->Self::Item;}async fn stored<T:Family>(){let value=T::make();ready(()).await;drop(value);}fn check<T:Family<Item=Rc<u32>>>(){send(stored::<T>());}fn main(){}",
            false,
        ),
        (
            "await_output_uninitialized",
            "struct Z;impl Future for Z{type Output=Rc<u32>;fn poll(self:std::pin::Pin<&mut Self>,_:&mut std::task::Context<'_>)->std::task::Poll<Self::Output>{std::task::Poll::Ready(Rc::new(7))}}fn main(){send(async{Z.await});}",
            true,
        ),
        (
            "cell_output_sync",
            "fn main(){sync(async {Cell::new(7)});}",
            true,
        ),
        (
            "held_cell_not_sync",
            "fn main(){sync(async {let x=Cell::new(7);ready(()).await;x.get()});}",
            false,
        ),
        (
            "captured_cell_not_sync",
            "fn main(){let x=Cell::new(7);sync(async move{x.get()});}",
            false,
        ),
        (
            "shared_cell_not_send",
            "fn main(){let x=Cell::new(7);send(async {x.get()});}",
            false,
        ),
        (
            "mutable_cell_send",
            "fn main(){let mut x=Cell::new(7);let p=&mut x;send(async move {p.set(9)});}",
            true,
        ),
        (
            "static_only_saved_state",
            "struct Token<'a>(&'a u32);unsafe impl Send for Token<'static>{}fn check<'a>(p:&'a u32){send(async move{let value=Token(p);ready(()).await;drop(value)});}fn main(){}",
            false,
        ),
        (
            "static_only_saved_state_behind_box",
            "struct Token<'a>(&'a u32);unsafe impl Send for Token<'static>{}fn check<'a>(p:&'a u32){send(async move{let value=Box::new(Token(p));ready(()).await;drop(value)});}fn main(){}",
            false,
        ),
        (
            "await_erased_send_future",
            "use std::pin::Pin;fn check<'a>(future:Pin<Box<dyn Future<Output=u32>+Send+'a>>)->impl Future<Output=u32>+Send+'a{async move{future.await}}fn main(){}",
            true,
        ),
        (
            "await_erased_future_without_send",
            "use std::pin::Pin;fn check<'a>(future:Pin<Box<dyn Future<Output=u32>+'a>>)->impl Future<Output=u32>+Send+'a{async move{future.await}}fn main(){}",
            false,
        ),
        (
            "await_future_with_inherited_send",
            "use std::pin::Pin;trait Transport:Send{}trait Erased:Future<Output=u32>+Transport{}fn check<'a>(future:Pin<Box<dyn Erased+'a>>)->impl Future<Output=u32>+Send+'a{async move{future.await}}fn main(){}",
            true,
        ),
        (
            "ambiguous_deferred_send_method",
            "trait A{fn choose(&self){}}trait B{fn choose(&self){}}impl<F:Future+Send>A for F{}impl<F:Future>B for F{}fn main(){let f=async{};f.choose();}",
            false,
        ),
        (
            "unpin_unchanged",
            "fn require<F:Future+Unpin>(_:F){}fn main(){require(async {});}",
            false,
        ),
        (
            "partial_move_retained_sibling",
            "struct Pair{a:Rc<u32>,b:Box<u32>}fn main(){send(async{let p=Pair{a:Rc::new(7),b:Box::new(9)};drop(p.b);ready(()).await;*p.a});}",
            false,
        ),
    ];
    let root = std::env::temp_dir().join(format!("flux-async-auto-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, accepted) in cases {
        let source = format!("{prelude}{body}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if accepted {
            assert!(
                typed.diagnostics.is_empty(),
                "{name}: {:?}",
                typed.diagnostics
            );
            assert!(
                !typed.sigs.pending_auto_trait_proofs,
                "{name}: unresolved pending flag"
            );
            assert!(
                typed
                    .bodies
                    .iter()
                    .map(|(_, result)| result)
                    .chain(typed.consts.iter().map(|(_, result)| result))
                    .all(|r| r.deferred_auto_traits.is_empty())
            );
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(mir.diagnostics.is_empty(), "{name}: {:?}", mir.diagnostics);
        } else {
            let mut errors = typed.diagnostics.clone();
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
            assert!(!errors.is_empty(), "flux accepted {name}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn type_only_proofs_fail_closed_at_build_and_export() {
    let (_, _, hir) = flux_hir::lower_text(
        "pending.rs",
        "fn require<F:std::future::Future+Send>(_:F){}fn main(){require(async{});}",
    );
    let typed = flux_ty::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    assert!(typed.sigs.pending_auto_trait_proofs);
    assert!(
        typed
            .bodies
            .iter()
            .map(|(_, result)| result)
            .any(|r| !r.deferred_auto_traits.is_empty())
    );
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(!mir.diagnostics.is_empty());
    assert!(
        typed
            .sigs
            .encode_flib(&mut flux_hir::flib::Encoder::new())
            .is_err()
    );
    let fork = typed.sigs.fork_for_extension();
    assert!(fork.pending_auto_trait_proofs);
    assert!(
        fork.encode_flib(&mut flux_hir::flib::Encoder::new())
            .is_err()
    );
    let goal = typed
        .bodies
        .iter()
        .flat_map(|(_, result)| result.deferred_auto_traits.iter())
        .next()
        .unwrap();
    let mut proved = flux_mir::check_file(&hir);
    assert!(proved.diagnostics.is_empty(), "{:?}", proved.diagnostics);
    assert!(
        proved
            .sigs
            .coroutine_witnesses
            .values()
            .any(|stored| stored.is_empty())
    );
    assert!(flux_solve::traits::holds(
        &hir.items,
        &proved.sigs,
        &goal.caller_bounds,
        goal.predicate
    ));
    let fork = proved.sigs.fork_for_extension();
    assert_eq!(
        fork.coroutine_witnesses.len(),
        proved.sigs.coroutine_witnesses.len()
    );
    proved.sigs.coroutine_witnesses.clear();
    assert!(
        !flux_solve::traits::holds(
            &hir.items,
            &proved.sigs,
            &goal.caller_bounds,
            goal.predicate
        ),
        "missing witness became an empty proof"
    );
}
