//! Impl headers keep projection inputs and the caller's rigid types distinct.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn array_projection_headers_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-array-projection-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let declarations = "trait Array { type Item; } impl<T> Array for [T; 4] { type Item=T; } struct Vector<A:Array>(A);";
    for (name, body, accepted) in [
        (
            "disjoint",
            "impl<A:Array> From<A> for Vector<A>{fn from(a:A)->Self{Vector(a)}} impl<A:Array> From<&[A::Item]> for Vector<A>{fn from(_: &[A::Item])->Self{panic!()}}",
            true,
        ),
        (
            "overlap",
            "impl<T> Array for &[T]{type Item=T;} impl<A:Array> From<A> for Vector<A>{fn from(a:A)->Self{Vector(a)}} impl<A:Array> From<&[A::Item]> for Vector<A>{fn from(_: &[A::Item])->Self{panic!()}}",
            false,
        ),
        (
            "header",
            "trait Read<T:?Sized>{} impl<A:Array> Read<[A::Item]> for Vector<A>{}",
            true,
        ),
        (
            "self_projection",
            "trait Read{} impl<A:Array> Read for [A::Item]{}",
            false,
        ),
        (
            "bound_call",
            "trait Read<T>{fn read()->u64;} impl<A:Array,B> Read<(A::Item,B)> for (A,B){fn read()->u64{7}} fn f<T>()->u64{<([T;4],u64) as Read<(T,u64)>>::read()}",
            true,
        ),
        (
            "rigid_mismatch",
            "trait Read<T>{fn read()->u64;} impl<A:Array,B> Read<(A::Item,B)> for (A,B){fn read()->u64{7}} fn f<T>()->u64{<([T;4],u64) as Read<(u64,u64)>>::read()}",
            false,
        ),
        (
            "missing_bound",
            "trait Read<T:?Sized>{} impl<A> Read<[A::Item]> for A{}",
            false,
        ),
        (
            "generic_index",
            "fn get<T,I:core::slice::SliceIndex<[T]>>(s:&[T],i:I)->&I::Output{&s[i]}",
            true,
        ),
        (
            "escaping_index",
            "fn get<'a,I:core::slice::SliceIndex<[u64]>>(i:I)->&'a I::Output{let a=[1,2,3];&a[..][i]}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{declarations} {body} fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
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
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
