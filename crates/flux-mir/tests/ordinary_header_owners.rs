//! Projected impl headers retain independently represented trait-owner lifetimes.
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;

#[test]
fn ordinary_header_owners_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-header-owners-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "access_owner",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn f<'a>(x:&'a u32)->&'a u32{W(S(x)).conv(x)}fn main(){}"###,
            true,
        ),
        (
            "access_escape",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn f<'a>(x:&'a u32)->&'static u32{W(S(x)).conv(x)}fn main(){}"###,
            false,
        ),
        (
            "access_wrong_owner",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn f<'a,'b>(x:&'a u32,y:&'b u32)->&'b u32{W(S(x)).conv(y)}fn main(){}"###,
            false,
        ),
        (
            "explicit_static_owner",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn call_static<A:Conv<'static,E>,E>(a:A,e:E)->E{a.conv(e)}fn f<'a>(x:&'a u32)->&'a u32{call_static(W(S(x)),x)}fn main(){}"###,
            false,
        ),
        (
            "independent_owner",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'owner,'a>Access<'owner>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn call_static<A:Conv<'static,E>,E>(a:A,e:E)->E{a.conv(e)}fn f<'a>(x:&'a u32)->&'a u32{call_static(W(S(x)),x)}fn main(){}"###,
            true,
        ),
        (
            "owner_pair",
            r###"trait Source<'a>{type Out;}struct S<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Source<'a>for S<'a,'b>{type Out=(&'a u32,&'b u32);}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Source<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a,'b>(x:&'a u32,y:&'b u32)->(&'a u32,&'b u32){W(S(x,y)).take((x,y))}fn main(){}"###,
            true,
        ),
        (
            "owner_pair_swapped",
            r###"trait Source<'a>{type Out;}struct S<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Source<'a>for S<'a,'b>{type Out=(&'a u32,&'b u32);}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Source<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a,'b>(x:&'a u32,y:&'b u32)->(&'b u32,&'a u32){W(S(x,y)).take((x,y))}fn main(){}"###,
            false,
        ),
        (
            "owner_mutable",
            r###"trait Source<'a>{type Out;}struct S<'a>(&'a mut &'a u32);impl<'a>Source<'a>for S<'a>{type Out=&'a mut &'a u32;}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Source<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a>(s:S<'a>,x:&'a mut &'a u32)->&'a mut &'a u32{W(s).take(x)}fn main(){}"###,
            true,
        ),
        (
            "owner_mutable_invariant",
            r###"trait Source<'a>{type Out;}struct S<'a>(&'a mut &'a u32);impl<'a>Source<'a>for S<'a>{type Out=&'a mut &'a u32;}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Source<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a,'b>(s:S<'a>,x:&'a mut &'a u32)->&'a mut &'b u32{W(s).take(x)}fn main(){}"###,
            false,
        ),
        (
            "owner_static_bound",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a:'static>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn f(x:&'static u32)->&'static u32{W(S(x)).conv(x)}fn main(){}"###,
            true,
        ),
        (
            "owner_static_bound_missing",
            r###"trait Access<'a>{type Error;}struct S<'a>(&'a u32);impl<'a:'static>Access<'a>for S<'a>{type Error=&'a u32;}struct W<A>(A);trait Conv<'a,E>{fn conv(self,e:E)->E;}impl<'a,A:Access<'a>>Conv<'a,A::Error>for W<A>{fn conv(self,e:A::Error)->A::Error{e}}fn f<'a>(x:&'a u32)->&'a u32{W(S(x)).conv(x)}fn main(){}"###,
            false,
        ),
        (
            "owner_conditional",
            r###"trait Mark{}trait Access<'a>{type Out;}struct S<'a,T>(&'a u32,T);impl<'a,T:Mark>Access<'a>for S<'a,T>{type Out=&'a u32;}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Access<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a,T:Mark>(x:&'a u32,t:T)->&'a u32{W(S(x,t)).take(x)}fn main(){}"###,
            true,
        ),
        (
            "owner_conditional_missing",
            r###"trait Mark{}trait Access<'a>{type Out;}struct S<'a,T>(&'a u32,T);impl<'a,T:Mark>Access<'a>for S<'a,T>{type Out=&'a u32;}struct W<A>(A);trait Take<O>{fn take(self,o:O)->O;}impl<'a,A:Access<'a>>Take<A::Out>for W<A>{fn take(self,o:A::Out)->A::Out{o}}fn f<'a,T>(x:&'a u32,t:T)->&'a u32{W(S(x,t)).take(x)}fn main(){}"###,
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rust = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("r.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rust.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&rust.stderr)
        );
        let mut db = SourceDb::new();
        let file = db.add_with_edition(format!("/header-owners/{name}.rs"), source, Edition::E2024);
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
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
