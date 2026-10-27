//! Concrete Self enum variants remain variants inside trait implementations.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn self_enum_variants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-self-enum-variants-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "from_str",
            "use std::str::FromStr;enum E{A,B}impl FromStr for E{type Err=();fn from_str(s:&str)->Result<Self,()>{Ok(match s{\"a\"=>Self::A,_=>Self::B})}}fn main(){assert!(E::from_str(\"a\").is_ok());}",
            true,
        ),
        (
            "clone_patterns",
            "enum E{A,B(u32)}impl Clone for E{fn clone(&self)->Self{match self{Self::A=>Self::A,Self::B(n)=>Self::B(*n)}}}fn main(){let _=E::A.clone();}",
            true,
        ),
        (
            "generic",
            "enum E<T>{Empty,Value(T),Record{value:T}}trait Make<T>{fn make(value:T)->Self;fn empty()->Self;fn take(self)->Option<T>;}impl<T> Make<T> for E<T>{fn make(value:T)->Self{Self::Value(value)}fn empty()->Self{Self::Empty}fn take(self)->Option<T>{match self{Self::Empty=>None,Self::Value(v)|Self::Record{value:v}=>Some(v)}}}fn main(){let e=E::make(7);assert_eq!(e.take(),Some(7));}",
            true,
        ),
        (
            "alias",
            "enum E<T>{A,B(T)}type Alias<T>=E<T>;trait Make{fn make()->Self;}impl Make for Alias<u32>{fn make()->Self{Self::A}}fn main(){let _:E<u32>=Alias::make();}",
            true,
        ),
        (
            "associated_constant",
            "enum E{A,B}trait Read{const N:u32;fn n(n:u32)->bool;}impl Read for E{const N:u32=7;fn n(n:u32)->bool{match n{Self::N=>true,_=>false}}}fn main(){assert!(E::n(7));}",
            true,
        ),
        (
            "missing",
            "enum E{A}trait Make{fn make()->Self;}impl Make for E{fn make()->Self{Self::Missing}}fn main(){}",
            false,
        ),
        (
            "abstract_self",
            "trait Make:Sized{fn make()->Self{Self::A}}fn main(){}",
            false,
        ),
        (
            "c_void_self_value",
            "trait Make{fn make()->Self;}impl Make for std::ffi::c_void{fn make()->Self{Self::__variant1}}fn main(){}",
            false,
        ),
        (
            "c_void_self_pattern",
            "trait Read{fn read(&self)->u32;}impl Read for std::ffi::c_void{fn read(&self)->u32{match self{Self::__variant1=>1,Self::__variant2=>2}}}fn main(){}",
            false,
        ),
        (
            "record_as_value",
            "enum E{A{x:u32}}trait Make{fn make()->Self;}impl Make for E{fn make()->Self{Self::A}}fn main(){}",
            false,
        ),
        (
            "tuple_arity",
            "enum E{A(u32,u32)}trait Make{fn make()->Self;}impl Make for E{fn make()->Self{Self::A(7)}}fn main(){}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
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
}
