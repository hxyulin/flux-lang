//! Exercise actual MIR constraints while the public declaration gate stays closed.
use std::process::Command;

#[test]
fn impl_header_regions_constrain_obligations_like_rustc() {
    let cases = [
        (
            "impl_owner_bound",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} struct Wrap<T>(T); impl<'a,T:Tr<'a>> Wrap<T> { fn need(self,_:&'a i32) {} } fn f<'a>(x:&'a i32) {Wrap(S).need(x);}",
            false,
        ),
        (
            "static_ok",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a i32) {} fn f(x:&'static i32) {need(S,x);}",
            true,
        ),
        (
            "static_unused",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
        (
            "repeated_ok",
            "trait Tr<'a,'b> {} struct S; impl<'x> Tr<'x,'x> for S {} fn need<'a,'b,T:Tr<'a,'b>>(_:T,_:&'a mut &'a i32,_:&'b mut &'b i32) {} fn f<'a>(x:&'a mut &'a i32,y:&'a mut &'a i32) {need(S,x,y);}",
            true,
        ),
        (
            "repeated_bad",
            "trait Tr<'a,'b> {} struct S; impl<'x> Tr<'x,'x> for S {} fn need<'a,'b,T:Tr<'a,'b>>(_:T,_:&'a mut &'a i32,_:&'b mut &'b i32) {} fn f<'a,'b>(x:&'a mut &'a i32,y:&'b mut &'b i32) {need(S,x,y);}",
            false,
        ),
        (
            "bounded_ok",
            "trait Tr<'a> {} struct S; impl<'a:'b,'b> Tr<'a> for &'b S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b>(s:&'b S,x:&'a mut &'a i32) {need::<&'b S>(s,x);}",
            true,
        ),
        (
            "bounded_bad",
            "trait Tr<'a> {} struct S; impl<'a:'b,'b> Tr<'a> for &'b S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b>(s:&'b S,x:&'a mut &'a i32) {need::<&'b S>(s,x);}",
            false,
        ),
        (
            "self_header_equality",
            "trait Tr<'a> {} struct Node<'a>(&'a i32); impl<'a> Tr<'a> for Node<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b>(n:Node<'b>,x:&'a mut &'a i32) {need(n,x);}",
            false,
        ),
        (
            "trait_type_argument",
            "trait Tr<T> {} struct S; impl Tr<&'static i32> for S {} fn need<'a,T:Tr<&'a i32>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
        (
            "recursive_bound",
            "trait Tr<'a> {} trait Outer<'a> {} struct S; impl Tr<'static> for S {} impl<'a,T:Tr<'a>> Outer<'a> for T {} fn need<'a,T:Outer<'a>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
    ];
    check_cases(&cases, "impl");
}

fn check_cases(cases: &[(&str, &str, bool)], group: &str) {
    let dir = std::env::temp_dir().join(format!(
        "flux-trait-region-obligations-{}-{group}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for &(name, source, accepts) in cases {
        let source = format!("{source} fn main() {{}}");
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.iter().all(|d| d.message
                == "lifetime parameters on traits are not supported yet"
                || d.message.starts_with("trait takes 0 lifetime arguments")
                || d.message
                    == "lifetimes in the generic arguments of bounds are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
        if !source.contains("trait Tr<T>") {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message == "lifetime parameters on traits are not supported yet"),
                "declaration guard must remain closed"
            );
        }
        // Keep the declaration diagnostics intact: only inspect the independent
        // borrow-checking result, never treating this as public acceptance.
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepts,
            "{name}: {:?}",
            mir.diagnostics
        );
        if !accepts {
            assert!(
                mir.diagnostics
                    .iter()
                    .any(|d| d.message.contains("lifetime") || d.message.contains("escapes")),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn caller_bounds_prove_regions_like_rustc() {
    check_cases(
        &[
            (
                "caller_matching",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<'a>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                true,
            ),
            (
                "caller_distinct",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<'a>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                false,
            ),
            (
                "caller_mutual",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b:'a,T:Tr<'a>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                true,
            ),
            (
                "caller_one_way",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b,T:Tr<'a>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                false,
            ),
            (
                "caller_reverse",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b:'a,T:Tr<'a>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                false,
            ),
            (
                "caller_static_ok",
                "trait Tr<'a> {} fn need<T:Tr<'static>>(_:T) {} fn f<T:Tr<'static>>(t:T) {need(t);}",
                true,
            ),
            (
                "caller_static_bad",
                "trait Tr<'a> {} fn need<T:Tr<'static>>(_:T) {} fn f<'a,T:Tr<'a>>(t:T) {need(t);}",
                false,
            ),
            (
                "caller_static_bound",
                "trait Tr<'a> {} fn need<T:Tr<'static>>(_:T) {} fn f<'a:'static,T:Tr<'a>>(t:T) {need(t);}",
                true,
            ),
            (
                "caller_repeated_ok",
                "trait Tr<'a,'b> {} fn need<'a,T:Tr<'a,'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<'a,'a>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                true,
            ),
            (
                "caller_repeated_bad",
                "trait Tr<'a,'b> {} fn need<'a,T:Tr<'a,'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<'a,'b>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                false,
            ),
            (
                "caller_unused_bad",
                "trait Tr<'a,'b> {} fn need<'a,T:Tr<'a,'static>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<'a,'b>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                false,
            ),
            (
                "caller_candidates",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,U:Tr<'a>,T:Tr<'b>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                true,
            ),
            (
                "caller_candidates_reverse",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'b,T:Tr<'b>+Tr<'b>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                true,
            ),
            (
                "caller_candidates_bad",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,'c,U:Tr<'a>,T:Tr<'b>>(t:T,x:&'c mut &'c i32) {need(t,x);}",
                false,
            ),
            (
                "caller_nested_type_ok",
                "trait Tr<T> {} fn need<'a,T:Tr<(&'a i32,&'static i32)>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<(&'a i32,&'static i32)>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                true,
            ),
            (
                "caller_nested_type_bad",
                "trait Tr<T> {} fn need<'a,T:Tr<(&'a i32,&'static i32)>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<(&'a i32,&'b i32)>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                false,
            ),
            (
                "caller_type_one_way",
                "trait Tr<T> {} fn need<'a,T:Tr<&'a i32>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b,T:Tr<&'a i32>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                false,
            ),
            (
                "caller_transitive_mutual",
                "trait Tr<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b:'c,'c:'a,T:Tr<'a>>(t:T,x:&'c mut &'c i32) {need(t,x);}",
                true,
            ),
            (
                "caller_type_ok",
                "trait Tr<T> {} fn need<'a,T:Tr<&'a i32>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<&'a i32>>(t:T,x:&'a mut &'a i32) {need(t,x);}",
                true,
            ),
            (
                "caller_type_bad",
                "trait Tr<T> {} fn need<'a,T:Tr<&'a i32>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<&'a i32>>(t:T,x:&'b mut &'b i32) {need(t,x);}",
                false,
            ),
            (
                "caller_owner_ok",
                "trait Tr<'a> {} struct W<T>(T); fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} impl<'a,T:Tr<'a>> W<T> {fn f(self,x:&'a mut &'a i32) {need(self.0,x);}}",
                true,
            ),
            (
                "caller_owner_bad",
                "trait Tr<'a> {} struct W<T>(T); fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} impl<'a,T:Tr<'a>> W<T> {fn f<'b>(self,x:&'b mut &'b i32) {need(self.0,x);}}",
                false,
            ),
            (
                "caller_forward_ok",
                "trait Tr<'a> {} trait Outer<'a> {} struct W<T>(T); impl<'a,T:Tr<'a>> Outer<'a> for W<T> {} fn need<'a,T:Outer<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<'a>>(t:T,x:&'a mut &'a i32) {need(W(t),x);}",
                true,
            ),
            (
                "caller_forward_bad",
                "trait Tr<'a> {} trait Outer<'a> {} struct W<T>(T); impl<'a,T:Tr<'a>> Outer<'a> for W<T> {} fn need<'a,T:Outer<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Tr<'a>>(t:T,x:&'b mut &'b i32) {need(W(t),x);}",
                false,
            ),
            (
                "caller_alias_ok",
                "trait Tr<'a> {} trait Has {type Item;} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Has>(t:T::Item,x:&'a mut &'a i32) where T::Item:Tr<'a> {need(t,x);}",
                true,
            ),
            (
                "caller_alias_bad",
                "trait Tr<'a> {} trait Has {type Item;} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b,T:Has>(t:T::Item,x:&'b mut &'b i32) where T::Item:Tr<'a> {need(t,x);}",
                false,
            ),
        ],
        "caller",
    );
}

#[test]
fn caller_metadata_fails_closed_and_candidates_are_independent() {
    use flux_ty::regions::Region;

    let source = "trait Tr<'r> {} fn need<'r,T:Tr<'r>>(_:T,_:&'r mut &'r i32) {} fn f<'a,'b,T:Tr<'b>+Tr<'b>>(t:T,x:&'b mut &'b i32) {need(t,x);} fn main() {}";
    // The unmodified program is Rust-valid. Mutations below are robustness
    // probes of internal metadata, not claims of Rust source acceptance.
    for mutation in 0..6 {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        let mut typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message == "lifetime parameters on traits are not supported yet")
        );
        let f = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "f")
            .unwrap()
            .0;
        let need = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "need")
            .unwrap()
            .0;
        let mut caller = typed.sigs.regions.fns[f].clone();
        let mut callee = typed.sigs.regions.fns[need].clone();
        match mutation {
            // Reject the first erased-equal candidate, then select the second.
            0 => caller.bound_traits[0].lifetimes[0] = Region::Param(0),
            1 => caller.bound_traits = Box::default(),
            2 => {
                for t in &mut caller.bound_traits {
                    t.lifetimes = Box::default();
                }
            }
            3 => {
                for t in &mut caller.bound_traits {
                    t.lifetimes[0] = Region::Erased;
                }
            }
            4 => callee.bound_traits = Box::default(),
            5 => {
                for t in &mut caller.bound_traits {
                    t.lifetimes[0] = Region::Param(100);
                }
            }
            _ => unreachable!(),
        }
        typed.sigs.regions.fns.insert(f, caller);
        typed.sigs.regions.fns.insert(need, callee);
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            mutation == 0,
            "mutation {mutation}: {:?}",
            mir.diagnostics
        );
    }
}

#[test]
fn caller_type_slot_metadata_is_required() {
    use flux_ty::regions::Region;
    let source = "trait Tr<T> {} fn need<'a,T:Tr<(&'a i32,&'static i32)>>(_:T,_:&'a mut &'a i32) {} fn f<'a,T:Tr<(&'a i32,&'static i32)>>(t:T,x:&'a mut &'a i32) {need(t,x);} fn main() {}";
    for mutation in 0..5 {
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        let mut typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.iter().all(|d| d.message
                == "lifetimes in the generic arguments of bounds are not supported yet")
        );
        let owner = if mutation == 4 { "need" } else { "f" };
        let f = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == owner)
            .unwrap()
            .0;
        let mut regions = typed.sigs.regions.fns[f].clone();
        match mutation {
            0 => {}
            1 => regions.bound_traits[0].args = Box::default(),
            2 => regions.bound_traits[0].args[0] = Box::default(),
            3 | 4 => regions.bound_traits[0].args[0][0] = Region::Erased,
            _ => unreachable!(),
        }
        typed.sigs.regions.fns.insert(f, regions);
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            mutation == 0,
            "mutation {mutation}: {:?}",
            mir.diagnostics
        );
    }
}
