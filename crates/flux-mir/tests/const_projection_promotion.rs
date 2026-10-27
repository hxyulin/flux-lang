//! Constant tuple names and builtin projections qualify inside promoted aggregate tables.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn constant_projection_promotions_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-const-projection-promotion-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "dwarf_table",
            "const NAME:(u8,u8)=(3,8);const PC:[(u8,u8);2]=[(17,1),(18,6)];const TABLE:&[(u8,bool,&[(u8,u8)])]=&[(17,true,&[NAME,PC[0],PC[1]])];fn main(){}",
            true,
        ),
        (
            "const_chain",
            "const A:u16=513;const B:(u16,u16)=(A,A);fn get()->&'static[(u16,u16)]{&[B,B]}fn main(){}",
            true,
        ),
        (
            "const_struct",
            "#[derive(Clone,Copy)]struct S{a:u16,b:[u16;2]}const S0:S=S{a:513,b:[7,9]};fn get()->&'static[u16]{&[S0.a,S0.b[1]]}fn main(){}",
            true,
        ),
        (
            "const_index",
            "const A:[u16;2]=[7,9];const I:usize=1;fn get()->&'static[u16]{&[A[I]]}fn main(){}",
            true,
        ),
        (
            "const_enum",
            "#[derive(Clone,Copy)]enum E{A,B(u16)}const A:(E,E)=(E::A,E::B(7));fn get()->&'static[E]{&[A.0,A.1]}fn main(){}",
            true,
        ),
        (
            "const_assoc",
            "struct S;impl S{const P:(u16,u16)=(513,7);}fn get()->&'static[u16]{&[S::P.0,S::P.1]}fn main(){}",
            true,
        ),
        (
            "local_const_scope",
            "fn get()->&'static[u16]{const A:u16=513;const B:(u16,u16)=(A,7);&[B.0]}fn main(){}",
            true,
        ),
        (
            "const_float",
            "const A:(f32,f64)=(-1.5,2.25);fn get()->&'static[f64]{&[A.1]}fn main(){}",
            true,
        ),
        (
            "const_ref",
            "const A:&u16=&513;fn get()->&'static[&'static u16]{&[A]}fn main(){}",
            true,
        ),
        (
            "runtime_index",
            "const A:[u16;2]=[7,9];fn get(i:usize)->&'static[u16]{&[A[i]]}fn main(){}",
            false,
        ),
        (
            "local_array",
            "fn get()->&'static[u16]{let a=[7,9];&[a[0]]}fn main(){}",
            false,
        ),
        (
            "overloaded_index",
            "struct S;impl std::ops::Index<usize> for S{type Output=u16;fn index(&self,_:usize)->&u16{&513}}fn get()->&'static[u16]{&[S[0]]}fn main(){}",
            false,
        ),
        (
            "overloaded_deref",
            "struct S;impl std::ops::Deref for S{type Target=u16;fn deref(&self)->&u16{&513}}fn get()->&'static[u16]{&[*S]}fn main(){}",
            false,
        ),
        (
            "bounds",
            "const A:[u16;2]=[7,9];const T:&[u16]=&[A[2]];fn main(){}",
            false,
        ),
        (
            "cycle",
            "const A:u16=B;const B:u16=A;const T:&[u16]=&[A];fn main(){}",
            false,
        ),
        (
            "const_drop",
            "struct D;impl Drop for D{fn drop(&mut self){}}const D0:D=D;fn get()->&'static[D]{&[D0]}fn main(){}",
            false,
        ),
        (
            "const_cell",
            "use std::cell::Cell;const C:Cell<u8>=Cell::new(7);fn get()->&'static[Cell<u8>]{&[C]}fn main(){}",
            false,
        ),
        (
            "mutable",
            "const A:u16=7;fn get()->&'static mut[u16]{&mut[A]}fn main(){}",
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
