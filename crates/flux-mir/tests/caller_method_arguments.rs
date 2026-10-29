//! Repeated caller bounds keep their distinct trait argument choices.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn caller_method_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-caller-methods-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "trait Acc<T>{fn add(&mut self,value:T);}trait Read<T>{fn read(&self)->T;}trait Build<T>{fn build()->Self;}";
    for (name, body, accepted) in [
        (
            "two_inputs",
            "fn f<N,E,O:Acc<N>+Acc<E>>(out:&mut O,n:N,e:E){out.add(n);out.add(e);}",
            true,
        ),
        (
            "reversed_bounds",
            "fn f<N,E,O:Acc<E>+Acc<N>>(out:&mut O,n:N,e:E){out.add(n);out.add(e);}",
            true,
        ),
        (
            "three_inputs",
            "fn f<A,B,C,O:Acc<A>+Acc<B>+Acc<C>>(out:&mut O,a:A,b:B,c:C){out.add(b);out.add(a);out.add(c);}",
            true,
        ),
        (
            "borrowed_inputs",
            "fn f<'a,'b,O:Acc<&'a u32>+Acc<&'b str>>(out:&mut O,n:&'a u32,e:&'b str){out.add(n);out.add(e);}",
            true,
        ),
        (
            "return_context",
            "fn f<N,E,O:Read<N>+Read<E>>(out:&O)->(N,E){let n:N=out.read();let e:E=out.read();(n,e)}",
            true,
        ),
        (
            "explicit_ufcs",
            "fn f<N,E,O:Acc<N>+Acc<E>>(out:&mut O,n:N,e:E){<O as Acc<E>>::add(out,e);<O as Acc<N>>::add(out,n);}",
            true,
        ),
        (
            "repeated_same_bound",
            "fn f<N,O:Acc<N>+Acc<N>>(out:&mut O,n:N){out.add(n);}",
            true,
        ),
        (
            "wrong_argument",
            "fn f<N,E,O:Acc<N>+Acc<E>>(out:&mut O){out.add(7u8);}",
            false,
        ),
        (
            "no_input_ambiguity",
            "fn f<N,E,O:Build<N>+Build<E>>()->O{O::build()}",
            false,
        ),
        (
            "no_output_context",
            "fn f<N,E,O:Read<N>+Read<E>>(out:&O){let _value=out.read();}",
            false,
        ),
        (
            "distinct_trait_ambiguity",
            "trait Other<T>{fn add(&mut self,t:T);}fn f<T,O:Acc<T>+Other<T>>(out:&mut O,t:T){out.add(t);}",
            false,
        ),
        (
            "static_input_not_weakened",
            "fn f<'a,O:Acc<&'static u32>>(out:&mut O,r:&'a u32){out.add(r);}",
            false,
        ),
        (
            "mutable_inner_invariance",
            "fn f<'a,'b,O:Acc<&'a mut &'b u32>>(out:&mut O,r:&'a mut &'a u32){out.add(r);}",
            false,
        ),
        (
            "borrowed_argument_escape",
            "fn f<O:Acc<&'static u32>+Acc<u8>>(out:&mut O){let r=7;out.add(&r);}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{prefix}{body}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
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
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}
