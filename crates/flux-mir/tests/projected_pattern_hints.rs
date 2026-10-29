//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn projected_pattern_hints_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-projected-pattern-hints-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "nested_tuple",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){assert_eq!(tupled(|(x,_)|match x{b'+'=>1,b'-'=>2,_=>0},Bytes(b\"+\")),1);}",
            true,
        ),
        (
            "optional",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){assert_eq!(optional(|(x,_)|match x{Some(b'+')=>1,Some(b'-')=>2,_=>0},Bytes(b\"-\")),2);}",
            true,
        ),
        (
            "reference_nested",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){let n=tupled(|pair|match &pair{(b'+',_)=>1,_=>0},Bytes(b\"+\"));assert_eq!(n,1);}",
            true,
        ),
        (
            "annotated",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){assert_eq!(annotated(|b:&[u8]|b.len(),Bytes(b\"abc\")),3);}",
            true,
        ),
        (
            "declared_return",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){assert_eq!(returned(||->u8{7},Bytes(b\"x\")),7);}",
            true,
        ),
        (
            "wrong_byte",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){let _=tupled(|(x,_)|match x{b'+'=>1,_=>0},Wide);}",
            false,
        ),
        (
            "wrong_option_byte",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){let _=optional(|(x,_)|match x{Some(b'+')=>1,_=>0},Wide);}",
            false,
        ),
        (
            "wrong_annotation",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){let _=annotated(|b:&[u16]|b.len(),Bytes(b\"abc\"));}",
            false,
        ),
        (
            "wrong_return",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn main(){let _=returned(||->u16{7},Bytes(b\"x\"));}",
            false,
        ),
        (
            "static_annotation",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn go<'a>(b:&'a[u8]){let _=annotated(|x:&'static[u8]|x.len(),Bytes(b));}fn main(){}",
            false,
        ),
        (
            "generic_pattern_abstract",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn go<I:Input>(i:I)->u8{tupled(|(x,_)|match x{b'+'=>1,_=>0},i)}fn main(){}",
            false,
        ),
        (
            "generic_annotation_abstract",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn go<I:Input>(i:I)->usize{annotated(|x:&[u8]|x.len(),i)}fn main(){}",
            false,
        ),
        (
            "generic_bound_pattern",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn go<I:Input<Token=u8>>(i:I)->u8{tupled(|(x,_)|match x{b'+'=>1,_=>0},i)}fn main(){}",
            true,
        ),
        (
            "generic_bound_annotation",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}fn go<'a,I:Input<Slice=&'a[u8]>>(i:I)->usize{annotated(|x:&[u8]|x.len(),i)}fn main(){}",
            true,
        ),
        (
            "declared_pointer_binder",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}type Poly=for<'a>fn(&'a u32)->&'a u32;struct CallbackInput;fn identity(x:&u32)->&u32{x}impl Input for CallbackInput{type Token=Poly;type Slice=&'static[u8];fn token(self)->Poly{identity}fn slice(self)->Self::Slice{&[]}}fn main(){let _=tupled(|(f,_):(Poly,u8)|{let n=9;*f(&n)as u8},CallbackInput);}",
            true,
        ),
        (
            "wrong_declared_pointer_binder",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}type Poly=for<'a>fn(&'a u32)->&'a u32;struct CallbackInput;fn identity(x:&u32)->&u32{x}impl Input for CallbackInput{type Token=Poly;type Slice=&'static[u8];fn token(self)->Poly{identity}fn slice(self)->Self::Slice{&[]}}fn main(){let _=tupled(|(_,_):(fn(&'static u32)->&'static u32,u8)|0,CallbackInput);}",
            false,
        ),
        (
            "wrong_return_pointer_binder",
            "trait Input{type Token;type Slice;fn token(self)->Self::Token;fn slice(self)->Self::Slice;}struct Bytes<'a>(&'a[u8]);impl<'a>Input for Bytes<'a>{type Token=u8;type Slice=&'a[u8];fn token(self)->u8{self.0[0]}fn slice(self)->Self::Slice{self.0}}struct Wide;impl Input for Wide{type Token=u16;type Slice=&'static[u8];fn token(self)->u16{7}fn slice(self)->Self::Slice{&[7]}}fn tupled<I:Input,F:FnMut((I::Token,u8))->u8>(mut f:F,i:I)->u8{f((i.token(),0))}fn optional<I:Input,F:FnMut((Option<I::Token>,u8))->u8>(mut f:F,i:I)->u8{f((Some(i.token()),0))}fn annotated<I:Input,F:FnMut(I::Slice)->usize>(mut f:F,i:I)->usize{f(i.slice())}fn returned<I:Input,F:FnMut()->I::Token>(mut f:F,_:I)->I::Token{f()}type Poly=for<'a>fn(&'a u32)->&'a u32;struct CallbackInput;fn identity(x:&u32)->&u32{x}impl Input for CallbackInput{type Token=Poly;type Slice=&'static[u8];fn token(self)->Poly{identity}fn slice(self)->Self::Slice{&[]}}fn main(){let _=returned(||->fn(&'static u32)->&'static u32{identity},CallbackInput);}",
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
