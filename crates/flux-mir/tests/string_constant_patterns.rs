//! Named shared string patterns use content equality and stay refutable.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

fn errors(name: &str, source: &str) -> Vec<String> {
    let mut db = SourceDb::new();
    let file = db.add_with_edition(
        format!("/string-patterns/{name}.rs"),
        source,
        Edition::E2021,
    );
    let hir = flux_hir::lower_sources(
        &mut db,
        &[SourceCrate {
            root: file,
            name: Symbol::intern(name),
            dependencies: vec![],
        }],
    );
    let typed = flux_ty::check_file(&hir);
    let mut errors: Vec<_> = hir
        .diagnostics
        .iter()
        .chain(&typed.diagnostics)
        .map(|d| d.message.clone())
        .collect();
    if errors.is_empty() {
        errors.extend(
            flux_mir::build_file(&hir, &typed)
                .diagnostics
                .into_iter()
                .map(|d| d.message),
        );
    }
    errors
}

#[test]
fn string_constants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-string-patterns-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "byte_view",
            r###"const A:&str=unsafe{std::str::from_utf8_unchecked(&[97,108,112,104,97])};fn tag(x:&str)->u8{match x{A=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "invalid_utf8",
            r###"const A:&str=unsafe{std::str::from_utf8_unchecked(&[255])};fn tag(x:&str)->u8{match x{A=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "associated",
            r###"struct Keys;impl Keys{const FIELD:&'static str="alpha";}fn tag(x:&str)->u8{match x{Keys::FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "trait_associated",
            r###"trait Keys{const FIELD:&'static str;}impl Keys for u32{const FIELD:&'static str="alpha";}fn tag(x:&str)->u8{match x{u32::FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "generic_associated",
            r###"trait Keys{const FIELD:&'static str;}fn tag<T:Keys>(x:&str)->u8{match x{T::FIELD=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "associated_function",
            r###"struct Keys;impl Keys{fn value()->&'static str{"alpha"}}fn tag(x:&str)->u8{match x{Keys::value=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "mixed_literals",
            r###"const A:&str="alpha";fn tag(x:&str)->u8{match x{A|"beta"=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "mutable_scrutinee",
            r###"const A:&str="alpha";fn tag(x:&mut str)->u8{match x{A=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "nested_refutable",
            r###"const A:&str="alpha";fn tag(x:(&str,u8)){let (A,_)=x;}fn main(){}"###,
            false,
        ),
        (
            "invalid_constant",
            r###"const A:&str=panic!("invalid string constant");fn tag(x:&str)->u8{match x{A=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "direct",
            r###"const FIELD:&str="alpha";fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "alias_type",
            r###"type Text=&'static str;const FIELD:Text="alpha";fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "alias_value",
            r###"const A:&str="alpha";const B:&str=A;fn tag(x:&str)->u8{match x{B=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "concat_value",
            r###"const FIELD:&str=concat!("al","pha");fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "constfn_value",
            r###"const fn value()->&'static str{"alpha"}const FIELD:&str=value();fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "constfn_choose",
            r###"const fn value(flag:bool)->&'static str{if flag{"alpha"}else{"beta"}}const FIELD:&str=value(false);fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "double_reference",
            r###"const FIELD:&str="alpha";fn tag(x:&&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "explicit_reference",
            r###"const FIELD:&str="alpha";fn tag(x:&&str)->u8{match x{&FIELD=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "option_ergonomics",
            r###"const FIELD:&str="alpha";fn tag(x:&Option<&str>)->u8{match x{Some(FIELD)=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "or_pattern",
            r###"const A:&str="alpha";const B:&str="beta";fn tag(x:&str)->u8{match x{A|B=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "shadow_binder",
            r###"const FIELD:&str="alpha";fn tag(x:&str)->u8{match x{other=>{let _=other;1}}}fn main(){}"###,
            true,
        ),
        (
            "nonexhaustive",
            r###"const FIELD:&str="alpha";fn tag(x:&str)->u8{match x{FIELD=>1}}fn main(){}"###,
            false,
        ),
        (
            "option_nonexhaustive",
            r###"const FIELD:&str="alpha";fn tag(x:Option<&str>)->u8{match x{Some(FIELD)=>1,None=>0}}fn main(){}"###,
            false,
        ),
        (
            "let_refutable",
            r###"const FIELD:&str="alpha";fn tag(x:&str){let FIELD=x;}fn main(){}"###,
            false,
        ),
        (
            "let_else",
            r###"const FIELD:&str="alpha";fn tag(x:&str)->u8{let FIELD=x else{return 0};1}fn main(){}"###,
            true,
        ),
        (
            "const_match",
            r###"const FIELD:&str="alpha";const fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "const_block_match",
            r###"const FIELD:&str="alpha";const HIT:u8=match "alpha"{FIELD=>1,_=>0};fn main(){}"###,
            false,
        ),
        (
            "runtime_variable",
            r###"fn tag(x:&str)->u8{let field="alpha";match x{field=>1,_=>0}}fn main(){}"###,
            true,
        ),
        (
            "static_pattern",
            r###"static FIELD:&str="alpha";fn tag(x:&str)->u8{match x{FIELD=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "function_pointer",
            r###"fn f(){}const F:fn()=f;fn tag(x:fn())->u8{match x{F=>1,_=>0}}fn main(){}"###,
            false,
        ),
        (
            "address_pointer",
            r###"const P:*const u8=&1u8;fn tag(x:*const u8)->u8{match x{P=>1,_=>0}}fn main(){}"###,
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let out = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let errors = errors(name, source);
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn other_reference_and_pointer_patterns_remain_unsupported() {
    for (name, source) in [
        (
            "null_pointer",
            r###"const P:*const u8=std::ptr::null();fn tag(x:*const u8)->u8{match x{P=>1,_=>0}}fn main(){}"###,
        ),
        (
            "scalar_reference",
            r###"const R:&u32=&3;fn tag(x:&u32)->u8{match x{R=>1,_=>0}}fn main(){}"###,
        ),
        (
            "byte_reference",
            r###"const R:&[u8;1]=b"a";fn tag(x:&[u8;1])->u8{match x{R=>1,_=>0}}fn main(){}"###,
        ),
    ] {
        let errors = errors(name, source);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("in patterns are not supported yet")),
            "{name}: {errors:?}"
        );
    }
}
