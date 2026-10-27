//! Inherited associated values keep their selected owner rows in callable bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn inherited_callable_arguments_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-inherited-callable-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "trait Base{type Item;}trait Child:Base{fn run<B,F>(&self,b:B,f:F)->B where F:FnMut(B,Self::Item)->B;}struct S<'a,'b>(&'a u32,&'b u32);impl<'a,'b>Base for S<'a,'b>{type Item=&'b u32;}";
    for (name, declaration, accepted) in [
        (
            "selected_second_row",
            "impl Child for S<'_,'_>{fn run<B,F>(&self,b:B,mut f:F)->B where F:FnMut(B,Self::Item)->B{f(b,self.1)}}",
            true,
        ),
        (
            "wrong_owner_row",
            "impl Child for S<'_,'_>{fn run<B,F>(&self,b:B,mut f:F)->B where F:FnMut(B,Self::Item)->B{f(b,self.0)}}",
            false,
        ),
        (
            "static_callback",
            "impl Child for S<'_,'_>{fn run<B,F>(&self,b:B,mut f:F)->B where F:FnMut(B,&'static u32)->B{f(b,self.1)}}",
            false,
        ),
        (
            "wrong_output",
            "impl Child for S<'_,'_>{fn run<B,F>(&self,b:B,mut f:F)->B where F:FnMut(B,Self::Item)->B{f(7,self.1)}}",
            false,
        ),
        (
            "nested_item",
            "trait B{type Item;}trait C:B{fn run<F>(&self,f:F)where F:FnMut(Self::Item);}struct X<'a,'b>(&'a u32,&'b u32);impl<'a,'b>B for X<'a,'b>{type Item=(&'b u32,Option<&'a u32>);}impl C for X<'_,'_>{fn run<F>(&self,mut f:F)where F:FnMut(Self::Item){f((self.1,Some(self.0)))}}",
            true,
        ),
        (
            "nested_wrong_row",
            "trait B{type Item;}trait C:B{fn run<F>(&self,f:F)where F:FnMut(Self::Item);}struct X<'a,'b>(&'a u32,&'b u32);impl<'a,'b>B for X<'a,'b>{type Item=(&'b u32,Option<&'a u32>);}impl C for X<'_,'_>{fn run<F>(&self,mut f:F)where F:FnMut(Self::Item){f((self.0,Some(self.1)))}}",
            false,
        ),
        (
            "captured_callback",
            "fn via<'a,F:FnMut(&'a u32)>(mut f:F,x:&'a u32){let mut nested=||f(x);nested();}",
            true,
        ),
        (
            "captured_wrong_row",
            "fn via<'a,'b,F:FnMut(&'a u32)>(mut f:F,x:&'b u32){let mut nested=||f(x);nested();}",
            false,
        ),
        (
            "impl_owner_callback",
            "struct Owner<'a,F:FnMut(&'a u32)>{f:F,x:&'a u32}impl<'a,F:FnMut(&'a u32)>Owner<'a,F>{fn go(&mut self){(self.f)(self.x)}}",
            true,
        ),
        (
            "no_borrow_callback",
            "fn via<F:FnMut(u32)>(mut f:F){let mut nested=||f(7);nested();}",
            true,
        ),
        (
            "iterator_rfold",
            "struct It<'a>(&'a u32);impl<'a>Iterator for It<'a>{type Item=&'a u32;fn next(&mut self)->Option<Self::Item>{None}}impl DoubleEndedIterator for It<'_>{fn next_back(&mut self)->Option<Self::Item>{None}fn rfold<B,F>(self,b:B,mut f:F)->B where F:FnMut(B,Self::Item)->B{f(b,self.0)}}",
            true,
        ),
        (
            "mutable_invariance",
            "trait B{type Item;}trait C:B{fn run<F>(self,f:F)where F:FnMut(Self::Item);}struct X<'a,'b>(&'a mut &'a u32,&'b mut &'b u32);impl<'a,'b>B for X<'a,'b>{type Item=&'b mut &'b u32;}impl C for X<'_,'_>{fn run<F>(self,mut f:F)where F:FnMut(Self::Item){f(self.0)}}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{prefix}{declaration}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
