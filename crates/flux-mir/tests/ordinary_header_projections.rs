//! Selected ordinary impl headers normalize outputs from original bound inputs.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn ordinary_header_projections_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-ordinary-header-projections-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "literal_shared",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(&'a str);impl<'a>Stream for Input<'a>{type Slice=&'a str;fn take(&mut self)->Self::Slice{self.0}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f<'a>(input:&mut Input<'a>)->Result<&'a str,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            true,
        ),
        (
            "literal_escape",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(&'a str);impl<'a>Stream for Input<'a>{type Slice=&'a str;fn take(&mut self)->Self::Slice{self.0}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f<'a>(input:&mut Input<'a>)->Result<&'static str,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            false,
        ),
        (
            "selected_static",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(&'a str);impl<'a:'static>Stream for Input<'a>{type Slice=&'a str;fn take(&mut self)->Self::Slice{self.0}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f(input:&mut Input<'static>)->Result<&'static str,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            true,
        ),
        (
            "selected_static_missing",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(&'a str);impl<'a:'static>Stream for Input<'a>{type Slice=&'a str;fn take(&mut self)->Self::Slice{self.0}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f<'a>(input:&mut Input<'a>)->Result<&'a str,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            false,
        ),
        (
            "literal_mutable",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(Option<&'a mut &'a u32>);impl<'a>Stream for Input<'a>{type Slice=&'a mut &'a u32;fn take(&mut self)->Self::Slice{self.0.take().unwrap()}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f<'a>(input:&mut Input<'a>)->Result<&'a mut &'a u32,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            true,
        ),
        (
            "literal_mut_invariance",
            r###"trait Stream{type Slice;fn take(&mut self)->Self::Slice;}
trait Compare<T>{}
trait Parser<I,O,E>{fn parse_next(&mut self,input:&mut I)->Result<O,E>;}
struct Literal<L>(L);
impl<L,I:Stream+Compare<L>,E>Parser<I,I::Slice,E>for Literal<L>{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{Ok(input.take())}}
fn literal<L,I,E>(value:L)->impl Parser<I,I::Slice,E>where I:Stream+Compare<L>{Literal(value)}
impl<'s,I:Stream+Compare<&'s str>,E>Parser<I,I::Slice,E>for &'s str{fn parse_next(&mut self,input:&mut I)->Result<I::Slice,E>{literal(*self).parse_next(input)}}

struct Input<'a>(Option<&'a mut &'a u32>);impl<'a>Stream for Input<'a>{type Slice=&'a mut &'a u32;fn take(&mut self)->Self::Slice{self.0.take().unwrap()}}impl<'a,'s>Compare<&'s str>for Input<'a>{}fn f<'a,'b>(input:&mut Input<'a>)->Result<&'a mut &'b u32,()>{let mut p="hello";Parser::parse_next(&mut p,input)}fn main(){}"###,
            false,
        ),
        (
            "header_occurrences",
            r###"trait Stream{type Slice;}struct Input<'a>(&'a str);impl<'a>Stream for Input<'a>{type Slice=&'a str;}trait Pair<I,A,B>{}impl<'s,I:Stream>Pair<I,I::Slice,I::Slice>for &'s str{}fn need<P:Pair<I,A,B>,I,A,B>(_:P){}fn f<'a>(_:&Input<'a>){need::<&str,Input<'a>,&'a str,&'a str>("literal")}fn main(){}"###,
            true,
        ),
        (
            "header_occurrences_wrong",
            r###"trait Stream{type Slice;}struct Input<'a>(&'a str);impl<'a>Stream for Input<'a>{type Slice=&'a str;}trait Pair<I,A,B>{}impl<'s,I:Stream>Pair<I,I::Slice,I::Slice>for &'s str{}fn need<P:Pair<I,A,B>,I,A,B>(_:P){}fn f<'a>(_:&Input<'a>){need::<&str,Input<'a>,&'a str,&'static str>("literal")}fn main(){}"###,
            false,
        ),
        (
            "missing_selection_input",
            r###"trait Stream{type Slice;}trait Parser<O>{}impl<I:Stream>Parser<I::Slice>for str{}fn main(){}"###,
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let out = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
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
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut diagnostics = hir.diagnostics.clone();
        if diagnostics.is_empty() {
            let typed = flux_ty::check_file(&hir);
            diagnostics.extend(typed.diagnostics.clone());
            if diagnostics.is_empty() {
                diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
