//! Expr fragment grouping preserves precedence and block semicolon elision.
#[test]
fn opaque_blocks_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-opaque-blocks-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, valid) in [
        (
            "block_stmt",
            "macro_rules! m{($e:expr)=>{{$e let x=1;let _=x;}}}fn main(){m!({let _=2;});}",
            true,
        ),
        (
            "paren_stmt_reject",
            "macro_rules! m{($e:expr)=>{{$e let x=1;}}}fn main(){m!(({let _=2;}));}",
            false,
        ),
        (
            "scalar_stmt_reject",
            "macro_rules! m{($e:expr)=>{{$e let x=1;}}}fn main(){m!(1);}",
            false,
        ),
        (
            "cfg_alternative",
            "macro_rules! m{($e:expr)=>{{#[cfg(all())]$e #[cfg(any())] false}}}fn main(){let _=m!({true});}",
            true,
        ),
        (
            "cfg_other",
            "macro_rules! m{($e:expr)=>{{#[cfg(any())]$e #[cfg(all())] true}}}fn main(){let _=m!({false});}",
            true,
        ),
        (
            "forwarded",
            "macro_rules! a{($e:expr)=>{b!($e)}}macro_rules! b{($e:expr)=>{{$e let _=1;}}}fn main(){a!({let _=2;});}",
            true,
        ),
        (
            "arithmetic",
            "macro_rules! m{($e:expr)=>{{$e + 1;}}}fn main(){m!({2});}",
            false,
        ),
        (
            "precedence",
            "macro_rules! m{($e:expr)=>{{$e * 3}}}fn main(){assert_eq!(m!(1+2),9);}",
            true,
        ),
        (
            "if_stmt",
            "macro_rules! m{($e:expr)=>{{$e let _=1;}}}fn main(){m!(if true {} else {});}",
            true,
        ),
        (
            "if_arithmetic",
            "macro_rules! m{($e:expr)=>{{$e + 1;}}}fn main(){m!(if true {2} else {3});}",
            false,
        ),
        (
            "block_postfix",
            "macro_rules! m{($e:expr)=>{{$e.len()}}}fn main(){let _=m!({[1,2]});}",
            true,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, body).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            valid,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, body);
        assert_eq!(
            hir.diagnostics.is_empty(),
            valid,
            "flux {name}: {:?}",
            hir.diagnostics
        );
    }
}
