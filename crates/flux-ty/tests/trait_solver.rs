//! Trait proof, declaration, and coherence regressions checked against rustc.
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Checks `source` with rustc as a library, from a temporary file.
fn rustc(name: &str, source: &str) -> Output {
    let stem = format!("flux-trait-solver-{}-{name}", std::process::id());
    let path = std::env::temp_dir().join(format!("{stem}.rs"));
    let metadata = path.with_extension("rmeta");
    std::fs::write(&path, source).unwrap();
    let output = Command::new("rustc")
        .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
        .args(["-A", "warnings"])
        .arg(&path)
        .arg("-o")
        .arg(&metadata)
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&metadata);
    let _ = std::fs::remove_file(&path);
    output
}

/// flux's type-checking diagnostics for `source`, which must lower cleanly.
fn flux(name: &str, source: &str) -> Vec<String> {
    let (_, _, hir) = flux_hir::lower_text(format!("{name}.rs"), source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_ty::check_file(&hir);
    typed.diagnostics.into_iter().map(|d| d.message).collect()
}

/// rustc reports `code` with a message starting with `message`, and flux
/// reports a diagnostic starting with the same text.
fn rejects(name: &str, source: &str, code: &str, message: &str) {
    let rustc = rustc(name, source);
    let stderr = String::from_utf8_lossy(&rustc.stderr);
    assert!(
        stderr.contains(&format!("error[{code}]: {message}")),
        "rustc {name}: expected {code} `{message}`, got:\n{stderr}"
    );
    let found = flux(name, source);
    assert!(
        found.iter().any(|m| m.starts_with(message)),
        "flux {name}: expected `{message}` ({code}), got {found:?}"
    );
}

/// rustc and flux both accept `source`.
fn accepts(name: &str, source: &str) {
    let rustc = rustc(name, source);
    assert!(
        rustc.status.success(),
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let found = flux(name, source);
    assert!(found.is_empty(), "flux {name}: {found:?}");
}

// Failure to prove a generic or upstream bound is not proof of disjointness.

#[test]
fn overlapping_blanket_impls() {
    rejects(
        "overlapping_blanket_impls",
        "trait Tr {}\nimpl<T: Copy> Tr for T {}\nimpl<T: Clone> Tr for T {}",
        "E0119",
        "conflicting implementations of trait `Tr`",
    );
}

#[test]
fn blanket_impl_overlaps_foreign_type() {
    rejects(
        "blanket_impl_overlaps_foreign_type",
        "trait Tr2 {}\nimpl<T: core::fmt::Display> Tr2 for T {}\nimpl Tr2 for Vec<u8> {}",
        "E0119",
        "conflicting implementations of trait `Tr2`",
    );
}

// Declaration proofs retain the caller's explicit and implicit Sized rules.

#[test]
fn impl_method_adds_sized_bound() {
    rejects(
        "impl_method_adds_sized_bound",
        "trait Tr { fn f<T: ?Sized>(x: &T) -> usize; }\n\
         impl Tr for u8 { fn f<T: ?Sized>(_: &T) -> usize where T: Sized { 0 } }",
        "E0276",
        "impl has stricter requirements than trait",
    );
}

#[test]
fn unsized_parameter_in_signature_bound() {
    rejects(
        "unsized_parameter_in_signature_bound",
        "trait Tr {}\nimpl<T> Tr for Box<T> {}\nstruct Need<X: Tr>(X);\n\
         fn g<U: ?Sized>(_: &Need<Box<U>>) {}",
        "E0277",
        "the size for values of type `U` cannot be known at compilation time",
    );
}

#[test]
fn drop_impl_requires_sized() {
    rejects(
        "drop_impl_requires_sized",
        "struct S<T: ?Sized>(Box<T>);\nimpl<T> Drop for S<T> { fn drop(&mut self) {} }",
        "E0367",
        "`Drop` impl requires `T: Sized` but the struct it is implemented for does not",
    );
}

// `u8: Tr` needs `W<u8>: Tr`, which needs `W<W<u8>>: Tr`, without end.
// Bodies, signatures and impl headers must preserve the overflow result.

const RECURSIVE: &str = "trait Tr { fn m(&self) -> u8 { 1 } }\nstruct W<T>(T);\n\
                         impl<T> Tr for T where W<T>: Tr {}\n";

#[test]
fn recursive_impl_in_body() {
    rejects(
        "recursive_impl_in_body",
        &format!("{RECURSIVE}fn need<T: Tr>() {{}}\npub fn f() {{ need::<u8>(); }}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
fn recursive_impl_in_method_call() {
    rejects(
        "recursive_impl_in_method_call",
        &format!("{RECURSIVE}pub fn f() -> u8 {{ 0u8.m() }}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
fn recursive_impl_in_signature() {
    rejects(
        "recursive_impl_in_signature",
        &format!("{RECURSIVE}struct Need<X: Tr>(X);\npub fn g(_: Need<u8>) {{}}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
fn recursive_impl_in_impl_where_clause() {
    rejects(
        "recursive_impl_in_impl_where_clause",
        &format!("{RECURSIVE}trait M {{}}\nimpl M for u8 where u8: Tr {{}}"),
        "E0275",
        "overflow evaluating the requirement",
    );
}

/// More nested adapters than the 32 goals at which `select_impl` stops.
/// Type checking accepts it; `tests/run/pending` has the whole program.
#[test]
fn deep_iterator_chain() {
    accepts(
        "deep_iterator_chain",
        include_str!("../../../tests/run/pending/deep_iterator_chain.rs"),
    );
}

/// `struct A0(A1, A1); ... struct A29(A30, A30);` is a DAG of 31 types.
/// `holds` explored it as a tree, so the `Sync` check of the static made
/// about 2^30 calls: 2.6 s at depth 22 and 9 s at depth 24 in a release
/// build. It proves each nested goal once now.
#[test]
fn shared_type_sync_check_is_bounded() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "sync_probe", "--nocapture"])
        .env("FLUX_SYNC_DEPTH", "30")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let limit = Duration::from_secs(10);
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "sync probe: {status}");
            return;
        }
        if start.elapsed() > limit {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("the Sync check of a depth-30 shared type took more than {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "subprocess entry point; run through the bounded parent"]
fn sync_probe() {
    let Ok(depth) = std::env::var("FLUX_SYNC_DEPTH") else {
        return;
    };
    let depth: usize = depth.parse().unwrap();
    let mut source = format!("struct A{depth};\n");
    for i in (0..depth).rev() {
        source.push_str(&format!("struct A{i}(A{}, A{});\n", i + 1, i + 1));
    }
    source.push_str("static S: Option<A0> = None;");
    let start = Instant::now();
    accepts("shared_type_sync_check", &source);
    println!("sync check at depth {depth}: {:?}", start.elapsed());
}

#[test]
fn contextual_impl_output_bindings() {
    for (name, source) in [
        (
            "conditional_nominal_output",
            "trait Adapter<A>{type Out;fn once(self,a:A)->Self::Out;}struct W<T>(T);impl<A,T>Adapter<A>for W<T>where A:Into<T>{type Out=T;fn once(self,a:A)->T{a.into()}}trait Child<A>:Adapter<A>{fn call(&self,a:A)->Self::Out;}impl<A,T>Child<A>for W<T>where Self:Adapter<A>{fn call(&self,a:A)->Self::Out{loop{}}}",
        ),
        (
            "callable_supertrait",
            "trait Once<A>{type Out;fn once(self,a:A)->Self::Out;} impl<T,A,R>Once<A>for T where T:FnOnce(A)->R{type Out=R;fn once(self,a:A)->R{self(a)}}trait Mut<A>:Once<A>{fn call(&mut self,a:A)->Self::Out;}impl<T,A,R>Mut<A>for T where T:FnMut(A)->R{fn call(&mut self,a:A)->R{self(a)}}",
        ),
        (
            "rigid_output_indices",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>>Child for T{fn get()->u32{7}}",
        ),
        (
            "separate_environments",
            "trait Source{type Out;}trait Base{type Out;}impl<R,T:Source<Out=R>>Base for T{type Out=R;}trait C32:Base{fn get()->Self::Out;}trait C64:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>>C32 for T{fn get()->u32{7}}impl<T:Source<Out=u64>>C64 for T{fn get()->u64{9}}",
        ),
        (
            "output_chain",
            "trait Source{type Out;}trait Base{type Out;}impl<T:Source<Out=U>,U:Source<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=U>,U:Source<Out=u32>>Child for T{fn get()->u32{7}}",
        ),
    ] {
        accepts(name, source);
    }
}

#[test]
fn contextual_impl_outputs_require_independent_evidence() {
    for (name, source) in [
        (
            "missing_prerequisite",
            "trait Source{type Out;}trait Marker{}trait Base{type Out;}impl<T:Source<Out=R>+Marker,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>>Child for T{fn get()->u32{7}}",
        ),
        (
            "conflicting_equalities",
            "trait Source{type Out;}trait Other{type Out;}trait Base{type Out;}impl<T:Source<Out=R>+Other<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>+Other<Out=u64>>Child for T{fn get()->u32{7}}",
        ),
        (
            "missing_requirement",
            "trait Source{type Out;}trait Marker{}trait Base{type Out;}impl<T:Source<Out=R>+Marker,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>+Base>Child for T{fn get()->u32{7}}",
        ),
        (
            "wrong_output",
            "trait Source{type Out;}trait Base{type Out;}impl<T:Source<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u64>>Child for T{fn get()->u32{7}}",
        ),
        (
            "abstract_membership",
            "trait Source{type Out;}trait Base{type Out;}impl<T:Source<Out=R>,R>Base for T{type Out=R;}trait Child:Base{fn get()->Self::Out;}impl<T:Source<Out=u32>+Base>Child for T{fn get()->u32{7}}",
        ),
    ] {
        let rustc = rustc(name, source);
        assert!(
            !rustc.status.success(),
            "rustc unexpectedly accepted {name}"
        );
        let found = flux(name, source);
        assert!(!found.is_empty(), "flux unexpectedly accepted {name}");
    }
}

/// Caller equalities must compare the goal and the assumption in the same form.
#[test]
fn rigid_caller_bounds_normalize_both_sides() {
    use flux_solve::traits::{self, RigidSolver};
    use flux_ty_ir::Ty;

    let source =
        "trait Source{type Out;}trait Mark<A>{}fn caller_proof<T:Source<Out=u32>,U:Mark<u32>>(){}";
    assert!(rustc("caller_bound_equivalence", source).status.success());
    let (_, _, hir) = flux_hir::lower_text("caller_bounds.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_ty::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let (function, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "caller_proof")
        .unwrap();
    let memberships = traits::elaborate(&typed.sigs, &typed.sigs.fn_predicates[function]);
    let projections = &typed.sigs.fn_projections[function];
    let assumption = *memberships
        .iter()
        .find(|p| p.trait_ref.name.as_str() == "Mark")
        .unwrap();
    let mut goal = assumption;
    goal.trait_ref.args = Ty::list(&[Ty::alias(projections[0].alias)]);
    let proof = RigidSolver::new(&hir.items, &typed.sigs, &memberships, projections, &[]);
    assert!(
        proof.holds(goal, &mut Vec::new()),
        "equivalent goal was not proven"
    );
    let mut wrong = goal;
    wrong.trait_ref.args = Ty::list(&[Ty::BOOL]);
    assert!(
        !proof.holds(wrong, &mut Vec::new()),
        "wrong argument was proven"
    );
    let without_mark: Vec<_> = memberships
        .iter()
        .copied()
        .filter(|p| *p != assumption)
        .collect();
    assert!(
        !RigidSolver::new(&hir.items, &typed.sigs, &without_mark, projections, &[])
            .holds(goal, &mut Vec::new()),
        "equality manufactured a missing membership"
    );
    let mut reversed = memberships.clone();
    *reversed.iter_mut().find(|p| **p == assumption).unwrap() = goal;
    assert!(
        RigidSolver::new(&hir.items, &typed.sigs, &reversed, projections, &[])
            .holds(assumption, &mut Vec::new()),
        "reverse comparison was not proven"
    );
    assert!(
        !proof
            .original_memberships_only()
            .holds(goal, &mut Vec::new()),
        "declaration proof rewrote its original memberships"
    );
}

#[test]
fn disjoint_local_and_unsized_impls() {
    accepts(
        "disjoint_local",
        "trait Tr{}struct Local;impl<T:Copy>Tr for T{}impl Tr for Local{}",
    );
    accepts(
        "disjoint_generic_local",
        "trait Tr{}struct Local<T>(T);impl<T:Copy>Tr for T{}impl<T>Tr for Local<T>{}",
    );
    accepts(
        "disjoint_unsized",
        "trait Tr{}impl<T>Tr for T{}impl Tr for str{}",
    );
}

#[test]
fn implicit_method_and_explicit_drop_sized_requirements() {
    rejects(
        "implicit_method_sized",
        "trait Tr{fn f<T:?Sized>(_:&T);}impl Tr for u8{fn f<T>(_:&T){}}",
        "E0276",
        "impl has stricter requirements than trait",
    );
    rejects(
        "explicit_drop_sized",
        "struct S<T:?Sized>(Box<T>);impl<T:?Sized>Drop for S<T>where T:Sized{fn drop(&mut self){}}",
        "E0367",
        "`Drop` impl requires `T: Sized` but the struct it is implemented for does not",
    );
    accepts(
        "matching_unsized_method",
        "trait Tr{fn f<T:?Sized>(_:&T);}impl Tr for u8{fn f<T:?Sized>(_:&T){}}",
    );
    accepts(
        "matching_unsized_drop",
        "struct S<T:?Sized>(Box<T>);impl<T:?Sized>Drop for S<T>{fn drop(&mut self){}}",
    );
    accepts(
        "unsized_signature_bound",
        "trait Tr{}impl<T:?Sized>Tr for Box<T>{}struct Need<X:Tr>(X);fn f<U:?Sized>(_:&Need<Box<U>>){}",
    );
    accepts(
        "permuted_drop_parameters",
        "struct S<A:?Sized,B>(Box<A>,B);impl<X,Y:?Sized>Drop for S<Y,X>{fn drop(&mut self){}}",
    );
    accepts(
        "sized_from_supertrait",
        "trait Tr{fn f<T:Copy+?Sized>(_:&T);}impl Tr for u8{fn f<T:Copy>(_:&T){}}",
    );
}

#[test]
fn rigid_overflow_status_is_query_local() {
    use flux_solve::traits::{Evaluation, RigidSolver};
    use flux_ty_ir::{Predicate, TraitRef, Ty};
    let source = "trait Loop{}impl<T>Loop for T where T:Loop{}trait Bound{}fn overflow_caller<T:Bound>(){}struct OverflowNode{next:Option<Box<OverflowNode>>}";
    assert!(rustc("overflow_query_context", source).status.success());
    let (_, _, hir) = flux_hir::lower_text("overflow_query.rs", source);
    let typed = flux_ty::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let (function, _) = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name.as_str() == "overflow_caller")
        .unwrap();
    let memberships = &typed.sigs.fn_predicates[function];
    let (loop_trait, data) = hir
        .items
        .traits
        .iter()
        .find(|(_, t)| t.name.as_str() == "Loop")
        .unwrap();
    let recursive = Predicate {
        self_ty: Ty::BOOL,
        trait_ref: TraitRef {
            trait_: loop_trait,
            name: data.name,
            args: &[],
        },
    };
    let proof = RigidSolver::new(&hir.items, &typed.sigs, memberships, &[], &[]);
    let (node, data) = hir
        .items
        .structs
        .iter()
        .find(|(_, s)| s.name.as_str() == "OverflowNode")
        .unwrap();
    let (_, &send) = hir
        .items
        .lang_traits
        .iter()
        .find(|(name, _)| name.as_str() == "send")
        .unwrap();
    let recursive_auto = Predicate {
        self_ty: Ty::adt(node, data.name, &[]),
        trait_ref: TraitRef {
            trait_: send,
            name: hir.items.traits[send].name,
            args: &[],
        },
    };
    assert_eq!(proof.evaluate(recursive_auto), Evaluation::Proven);
    assert_eq!(proof.evaluate(recursive), Evaluation::Overflow);
    let absent = Predicate {
        self_ty: Ty::BOOL,
        ..memberships[0]
    };
    assert_eq!(proof.evaluate(absent), Evaluation::Unproven);
    assert_eq!(proof.evaluate(memberships[0]), Evaluation::Proven);
    assert_eq!(proof.evaluate(recursive), Evaluation::Overflow);
    assert_eq!(proof.evaluate(recursive_auto), Evaluation::Proven);
    assert_eq!(
        flux_solve::traits::evaluate(&hir.items, &typed.sigs, &[], recursive),
        Evaluation::Overflow
    );
    assert_eq!(
        flux_solve::traits::evaluate(&hir.items, &typed.sigs, &[], absent),
        Evaluation::Unproven
    );
    assert_eq!(
        flux_solve::traits::evaluate(&hir.items, &typed.sigs, &[], recursive_auto),
        Evaluation::Proven
    );
}

#[test]
fn projection_signature_retains_unsized_parameters() {
    rejects(
        "unsized_projection_receiver",
        "trait Tr{type Out;}impl<T>Tr for Box<T>{type Out=();}fn f<U:?Sized>(_:&<Box<U>as Tr>::Out){}",
        "E0277",
        "the size for values of type `U` cannot be known at compilation time",
    );
    accepts(
        "unsized_projection_valid",
        "trait Tr{type Out;}impl<T:?Sized>Tr for Box<T>{type Out=();}fn f<U:?Sized>(_:&<Box<U>as Tr>::Out){}",
    );
}

#[test]
fn inductive_cycle_in_signature_reports_overflow() {
    rejects(
        "inductive_cycle_signature",
        "trait Tr{}impl<T>Tr for T where T:Tr{}struct Need<T:Tr>(T);fn f(_:Need<bool>){}",
        "E0275",
        "overflow evaluating the requirement",
    );
}

#[test]
fn fundamental_callable_bounds_preserve_disjointness() {
    for (name, ty) in [
        ("char", "char"),
        ("slice", "&[char]"),
        ("array", "[char; 4]"),
        ("foreign_nominal", "Vec<u8>"),
    ] {
        accepts(
            name,
            &format!("trait Tr{{}}impl<F:Fn(char)->bool>Tr for F{{}}impl Tr for {ty}{{}}"),
        );
    }
    rejects(
        "callable_reference_stays_open",
        "trait Tr{}impl<F:Fn(char)->bool>Tr for F{}impl<T>Tr for &T{}",
        "E0119",
        "conflicting implementations of trait `Tr`",
    );
    rejects(
        "callable_function_overlaps",
        "trait Tr{}impl<F:Fn(char)->bool>Tr for F{}impl Tr for fn(char)->bool{}",
        "E0119",
        "conflicting implementations of trait `Tr`",
    );
}

#[test]
fn projected_slice_headers_preserve_disjointness() {
    let header = "trait Ule{type Out;}struct Slice<T:Ule>([T::Out]);";
    let specific = "impl<T:Ule>AsRef<Slice<T>>for &[T::Out]{fn as_ref(&self)->&Slice<T>{loop{}}}";
    accepts("projected_slice_disjoint", &format!("{header}{specific}"));
    rejects(
        "projected_slice_existing_impl_overlaps",
        &format!(
            "{header}impl<T:Ule>AsRef<Slice<T>>for[T::Out]{{fn as_ref(&self)->&Slice<T>{{loop{{}}}}}}{specific}"
        ),
        "E0119",
        "conflicting implementations of trait `AsRef<Slice<_>>`",
    );
}

#[test]
fn generic_trait_objects_retain_declared_callable_capabilities() {
    accepts(
        "non_callable_parser_object",
        "trait Parser<I>{}impl<I,F:FnMut(&mut I)>Parser<I>for F{}impl<I>Parser<I>for Box<dyn Parser<I>>{}",
    );
    rejects(
        "callable_object_actual_overlap",
        "trait Parser<I>{}impl<I,F:FnMut(&mut I)>Parser<I>for F{}impl<I>Parser<I>for Box<dyn FnMut(&mut I)>{}",
        "E0119",
        "conflicting implementations of trait `Parser<_>`",
    );
}
