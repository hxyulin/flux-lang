//! Enum constants select their discriminant and active structural payload.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn enum_constant_patterns_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-enum-constant-patterns-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "unit_exhaustive",
            "#[derive(PartialEq)]enum E{A,B}const A:E=E::A;const B:E=E::B;fn f(x:E){match x{A=>{},B=>{}}}fn main(){}",
            true,
        ),
        (
            "tuple_payload",
            "#[derive(PartialEq)]enum E{A(u32),B}const A:E=E::A(3);fn f(x:E)->bool{matches!(x,A)}fn main(){}",
            true,
        ),
        (
            "record_payload",
            "#[derive(PartialEq)]enum E{A{x:u32,b:bool},B}const A:E=E::A{x:3,b:true};fn f(x:E)->bool{matches!(x,A)}fn main(){}",
            true,
        ),
        (
            "nested_arrays",
            "#[derive(PartialEq)]enum E{A([Option<u32>;2]),B}const A:E=E::A([Some(3),None]);fn f(x:E)->bool{matches!(x,A)}fn main(){}",
            true,
        ),
        (
            "field_pattern",
            "#[derive(PartialEq)]enum E{A,B}const A:E=E::A;struct S{e:E}fn f(x:S){match x{S{e:A}=>{},_=>{}}}fn main(){}",
            true,
        ),
        (
            "let_else",
            "#[derive(PartialEq)]enum E{A(u32),B}const A:E=E::A(3);fn f(x:E){let A=x else{return;};}fn main(){}",
            true,
        ),
        (
            "inactive_nonstructural",
            "struct M(u32);impl PartialEq for M{fn eq(&self,_:&Self)->bool{true}}#[derive(PartialEq)]enum E{A,B(M)}const A:E=E::A;fn f(x:E)->bool{matches!(x,A)}fn main(){}",
            true,
        ),
        (
            "single_unit_irrefutable",
            "#[derive(PartialEq)]enum E{A}const A:E=E::A;fn f(x:E){let A=x;}fn main(){}",
            true,
        ),
        (
            "missing_variant",
            "#[derive(PartialEq)]enum E{A,B}const A:E=E::A;fn f(x:E){match x{A=>{}}}fn main(){}",
            false,
        ),
        (
            "missing_payload",
            "#[derive(PartialEq)]enum E{A(bool),B}const A:E=E::A(true);fn f(x:E){match x{A=>{},E::B=>{}}}fn main(){}",
            false,
        ),
        (
            "refutable_let",
            "#[derive(PartialEq)]enum E{A,B}const A:E=E::A;fn f(x:E){let A=x;}fn main(){}",
            false,
        ),
        (
            "manual_equality",
            "enum E{A,B}impl PartialEq for E{fn eq(&self,_:&Self)->bool{true}}const A:E=E::A;fn f(x:E)->bool{matches!(x,A)}fn main(){}",
            false,
        ),
        (
            "active_nonstructural",
            "struct M(u32);impl PartialEq for M{fn eq(&self,_:&Self)->bool{true}}#[derive(PartialEq)]enum E{A,B(M)}const B:E=E::B(M(3));fn f(x:E)->bool{matches!(x,B)}fn main(){}",
            false,
        ),
        (
            "generic_constant",
            "#[derive(PartialEq)]enum E<T>{A(T),B}struct S<T>(T);impl<T:Sized>S<T>{const C:E<T>=E::B;}fn f<T>(x:E<T>){match x{S::<T>::C=>{},_=>{}}}fn main(){}",
            false,
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
}
