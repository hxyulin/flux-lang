//! Qualified selected projections retain exact impl and method input rows.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
#[test]
fn checkpoint_signatures_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-checkpoint-signatures-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "borrowed_projection",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<T>Offset for &[T]{fn offset(&self,_:&Self)->usize{31}}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset(&self,_:&<&'a[T]as Stream>::Checkpoint)->usize{47}}fn main(){}",
            true,
        ),
        (
            "concrete_rhs",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset(&self,_:&Checkpoint<&'a[T],&'a[T]>)->usize{47}}fn main(){}",
            true,
        ),
        (
            "alpha_owner",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'data,T>Offset<<&'data[T]as Stream>::Checkpoint>for &'data[T]{fn offset(&self,_:&Checkpoint<&'data[T],&'data[T]>)->usize{47}}fn main(){}",
            true,
        ),
        (
            "static_selection",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<T>Offset<<&'static[T]as Stream>::Checkpoint>for &'static[T]{fn offset(&self,_:&Checkpoint<&'static[T],&'static[T]>)->usize{47}}fn main(){}",
            true,
        ),
        (
            "wrong_static",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset(&self,_:&Checkpoint<&'static[T],&'static[T]>)->usize{47}}fn main(){}",
            false,
        ),
        (
            "wrong_element",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset(&self,_:&Checkpoint<&'a[u8],&'a[u8]>)->usize{47}}fn main(){}",
            false,
        ),
        (
            "permuted_rhs",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Pair<'a,'b,T>(&'a[T],&'b[T]);impl<'x,'y,T>Stream for Pair<'x,'y,T>{type Checkpoint=Checkpoint<&'y[T],&'x[T]>;}trait Offset<S>{fn offset(&self,start:&S)->usize;}impl<'left,'right,T>Offset<<Pair<'left,'right,T>as Stream>::Checkpoint>for Pair<'left,'right,T>{fn offset(&self,_:&Checkpoint<&'right[T],&'left[T]>)->usize{61}}fn main(){}",
            true,
        ),
        (
            "wrong_permutation",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Pair<'a,'b,T>(&'a[T],&'b[T]);impl<'x,'y,T>Stream for Pair<'x,'y,T>{type Checkpoint=Checkpoint<&'y[T],&'x[T]>;}trait Offset<S>{fn offset(&self,start:&S)->usize;}impl<'left,'right,T>Offset<<Pair<'left,'right,T>as Stream>::Checkpoint>for Pair<'left,'right,T>{fn offset(&self,_:&Checkpoint<&'left[T],&'right[T]>)->usize{61}}fn main(){}",
            false,
        ),
        (
            "wrong_repeated_row",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);struct Pair<'a,'b,T>(&'a[T],&'b[T]);impl<'x,'y,T>Stream for Pair<'x,'y,T>{type Checkpoint=Checkpoint<&'y[T],&'x[T]>;}trait Offset<S>{fn offset(&self,start:&S)->usize;}impl<'left,'right,T>Offset<<Pair<'left,'right,T>as Stream>::Checkpoint>for Pair<'left,'right,T>{fn offset(&self,_:&Checkpoint<&'right[T],&'right[T]>)->usize{61}}fn main(){}",
            false,
        ),
        (
            "mutable_invariance",
            "trait Stream{type Checkpoint;}struct C<T>(T);impl<'a,'b>Stream for &'a mut &'b u8{type Checkpoint=C<Self>;}trait Offset<S>{fn offset(&self,start:&S);}impl<'a,'b>Offset<<&'a mut &'b u8 as Stream>::Checkpoint>for u32{fn offset(&self,_:&C<&'a mut &'a u8>) {}}fn main(){}",
            false,
        ),
        (
            "wrong_arity",
            "trait Stream{type Checkpoint;}struct Checkpoint<I,S>(I,core::marker::PhantomData<S>);impl<T>Stream for &[T]{type Checkpoint=Checkpoint<Self,Self>;}trait Offset<Start=Self>{fn offset(&self,start:&Start)->usize;}impl<'a,T>Offset<<&'a[T]as Stream>::Checkpoint>for &'a[T]{fn offset(&self,_:&Checkpoint<&'a[T]>)->usize{47}}fn main(){}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2021", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(&path, source, Edition::E2021);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[SourceCrate {
                root: file,
                name: Symbol::intern(name),
                dependencies: vec![],
            }],
        );
        let typed = flux_ty::check_file(&hir);
        let mut errors = hir.diagnostics.clone();
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
